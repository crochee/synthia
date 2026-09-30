//! Per-session controller that serializes prompt/steer/cancel operations
//! and ensures at most one `Agent::run` per session.
//!
//! # Module layout
//!
//! - `ops`: `SessionOp` / `SessionState` wire types.
//! - `run_stream`: the run-stream factory seam.
//! - `deps`: `RunDependencies` — what every run is handed.
//! - `SessionController` (this file): the shared handle.
//! - `inner`: the shared inner state + lifecycle gates.
//! - `run_task` / `persist`: the run task and its event sink,
//!   as method continuations of the inner state.
//! - `dispatch`: the operation loop.
//! - `run_log`: the run's durable log writer.

use std::{
    sync::{
        Arc,
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context as _, Result};
use synthia::{
    harness::AgentEvent,
    session::{SessionSink, manager::InputQueue as SessionInputQueue},
};
use tokio::sync::mpsc;

use crate::event_stream::EventBroadcaster;

mod deps;
mod dispatch;
mod inner;
mod ops;
mod persist;
mod pin;
pub(crate) mod run_log;
mod run_stream;
mod run_task;

pub use deps::RunDependencies;
use dispatch::run_controller_loop;
use inner::ControllerInner;
pub use ops::{SessionOp, SessionState};
pub use pin::PinnedProvider;
use pin::SubmittedOp;
pub(crate) use run_log::RunLog;
pub use run_stream::{AgentRunStreamFactory, RunStreamFactory};

/// Default idle timeout before the controller shuts itself down when
/// no run is active and there are no streaming subscribers.
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Shared handle to a running session controller.
pub struct SessionController {
    session_id: String,
    user_id: String,
    state: Arc<Mutex<SessionState>>,
    op_tx: mpsc::Sender<SubmittedOp>,
    broadcaster: EventBroadcaster,
    alive: Arc<AtomicBool>,
    /// R29-Phase-I: idempotency guard for
    /// [`SessionController::close`]. Set on the first call so
    /// a second call is a no-op.
    closed: AtomicBool,
    /// R11: shared with [`ControllerInner`] (same `Arc`
    /// interior) so publishes from the run loop reach
    /// subscribers of the outer handle.
    snapshot_bus: synthia::session::SnapshotBus,
}

impl SessionController {
    /// Spawn the background controller task and return a shared handle.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        user_id: impl Into<String>,
        session_id: impl Into<String>,
        queue: SessionInputQueue,
        session_store: Arc<dyn SessionSink>,
        deps: RunDependencies,
        idle_timeout: Duration,
        run_factory: Arc<dyn RunStreamFactory>,
    ) -> Arc<Self> {
        let user_id = user_id.into();
        let session_id = session_id.into();
        let broadcaster =
            EventBroadcaster::with_label(format!("{user_id}/{session_id}"));
        let (op_tx, op_rx) = mpsc::channel(64);
        let state = Arc::new(Mutex::new(SessionState::Idle));
        let alive = Arc::new(AtomicBool::new(true));
        // R11: one bus shared between the outer handle and the
        // inner loop (Clone shares the Arc interior).
        let snapshot_bus = synthia::session::SnapshotBus::new();
        // Cloned out before `deps` moves into the inner mutex: the
        // counters are the controller's write handle on the
        // process-wide totals.
        let usage = Arc::clone(&deps.usage_metrics);
        let subagent_router = deps.subagent_router.clone();

        let controller = Arc::new(Self {
            session_id: session_id.clone(),
            user_id: user_id.clone(),
            state: state.clone(),
            op_tx,
            broadcaster: broadcaster.clone(),
            alive: alive.clone(),
            closed: AtomicBool::new(false),
            snapshot_bus: snapshot_bus.clone(),
        });

        let inner = Arc::new(ControllerInner {
            session_id: controller.session_id.clone(),
            user_id: controller.user_id.clone(),
            state,
            queue,
            session_store,
            broadcaster,
            deps: parking_lot::Mutex::new(deps),
            usage,
            subagent_router,
            last_request_header: parking_lot::Mutex::new(None),
            idle_timeout,
            run_cancel: Mutex::new(None),
            run_factory,
            alive,
            snapshot_bus,
            pending_multimodal: parking_lot::Mutex::new(None),
            pending_provider: parking_lot::Mutex::new(None),
            shutdown_finalised: AtomicBool::new(false),
        });

        tokio::spawn(run_controller_loop(inner, op_rx));

        controller
    }

    /// Submit an operation to the serialized controller loop.
    ///
    /// The turn runs on the session's configured default provider.
    /// Callers that resolved a `model` selection use
    /// [`SessionController::submit_with_provider`] instead.
    pub async fn submit(&self, op: SessionOp) -> Result<()> {
        self.submit_with_provider(op, None).await
    }

    /// Submit an operation together with the provider that runs it.
    ///
    /// `provider` is `None` for the session's configured default —
    /// the same thing [`SessionController::submit`] does. The
    /// selection applies to *this* operation's turn only: it is
    /// consumed by the run the operation starts, so a later operation
    /// without one runs the default again, and two operations queued
    /// behind a running turn each keep their own provider.
    pub async fn submit_with_provider(
        &self,
        op: SessionOp,
        provider: Option<PinnedProvider>,
    ) -> Result<()> {
        let submitted = SubmittedOp { op, provider };
        self.op_tx
            .send(submitted)
            .await
            .context("session controller is shut down")?;
        Ok(())
    }

    /// Convenience helper to request cancellation of the current run.
    pub async fn cancel(&self) -> Result<()> {
        self.submit(SessionOp::Cancel { reason: None }).await
    }

    /// The session this controller owns.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Current controller state.
    pub fn state(&self) -> SessionState {
        *self.state.lock().expect("state mutex poisoned")
    }

    /// Returns `true` while the background loop is still running.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Subscribe to the session's event broadcast channel.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<AgentEvent> {
        self.broadcaster.subscribe()
    }

    /// Push one event onto this session's broadcast channel.
    ///
    /// The sub-agent session router uses this to feed a child
    /// session's live SSE subscribers while the parent's run task
    /// drives the child — the child session's events originate in
    /// the parent's run, but its stream is its own.
    pub(crate) fn broadcast_event(&self, event: &AgentEvent) {
        if let Err(error) = self.broadcaster.send(event.clone()) {
            tracing::debug!(
                target: "synthia.session",
                session_id = %self.session_id,
                error = %error,
                "child session broadcast had no subscribers"
            );
        }
    }

    /// Close the session: signal every child run, let the
    /// controller loop stop, then finalise the durable log.
    ///
    /// Idempotent — a second call returns immediately. The
    /// internal wait for the controller loop is bounded by a
    /// **3-second** deadline so a hung run task cannot keep the
    /// process alive; on expiry the loop is abandoned and the
    /// caller proceeds. The terminal `lifecycle_shutdown` event
    /// and the sink close are performed by the loop itself (see
    /// `ControllerInner::finalise_shutdown`), after all children
    /// have been signalled.
    ///
    /// # Errors
    ///
    /// Returns `Err` only when the shutdown operation cannot be
    /// delivered to an already-stopped loop — in that case the
    /// session is already closed, so callers may treat the error
    /// as informational.
    pub async fn close(&self) -> Result<()> {
        if self.closed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        // Best-effort submit: if the loop already exited, its
        // receiver is gone and the session is closed anyway.
        let _ = self
            .op_tx
            .send(SubmittedOp {
                op: SessionOp::Shutdown,
                provider: None,
            })
            .await;
        match tokio::time::timeout(Duration::from_secs(3), self.op_tx.closed())
            .await
        {
            Ok(()) => {}
            Err(_) => {
                tracing::warn!(
                    target: "synthia.session",
                    session_id = %self.session_id,
                    "session close: controller loop did not stop within 3s"
                );
            }
        }
        Ok(())
    }

    /// Subscribe to the session's operation-snapshot bus (R11).
    ///
    /// Returns the receiver plus the latest snapshot published,
    /// so a late subscriber learns the current state without
    /// waiting for the next transition. HTTP `/status`, TUI,
    /// and replay consumers use this instead of polling
    /// [`SessionController::state`].
    pub fn subscribe_snapshots(
        &self,
    ) -> (
        synthia::session::SnapshotReceiver,
        Option<synthia::session::OperationSnapshot>,
    ) {
        self.snapshot_bus.subscribe()
    }
}

#[cfg(test)]
mod tests;
