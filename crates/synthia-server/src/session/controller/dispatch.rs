//! The controller loop: receiving operations, dispatching each to
//! its handler, and the idle-timeout lifecycle around them.

use std::{
    future::pending,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use anyhow::Result;
use serde_json::Value;
use synthia::{core::Clock, provider::traits::ModelProvider};
use tokio::sync::mpsc;

use super::{
    inner::{ControllerInner, ParkedPrompt},
    ops::{SessionOp, SessionState},
    pin::SubmittedOp,
    run_log::truncate,
};

pub(super) async fn run_controller_loop(
    inner: Arc<ControllerInner>,
    mut op_rx: mpsc::Receiver<SubmittedOp>,
) {
    let mut last_activity = tokio::time::Instant::now();
    let mut run_handle: Option<tokio::task::JoinHandle<()>> = None;
    // R29-Phase-I: the reason recorded on the terminal
    // `LifecycleShutdown` event. Each exit path overrides it;
    // the default (Completed) covers the channel-closed path.
    let mut exit_reason = synthia::session::SessionEndReason::Completed;

    loop {
        let idle_deadline =
            idle_deadline(&inner, &last_activity, run_handle.is_some()).await;

        tokio::select! {
            biased;

            Some(submitted) = op_rx.recv() => {
                last_activity = tokio::time::Instant::now();
                if handle_received_op(&inner, submitted, &mut run_handle).await {
                    exit_reason =
                        synthia::session::SessionEndReason::Interrupted;
                    break;
                }
            }

            result = wait_for_run(&mut run_handle), if run_handle.is_some() => {
                on_run_completed(&inner, result, &mut run_handle).await;
                last_activity = tokio::time::Instant::now();
            }

            _ = wait_idle_timeout(idle_deadline), if idle_deadline.is_some() => {
                handle_idle_timeout(&inner, &mut run_handle).await;
                break;
            }

            else => {
                join_run(run_handle.take()).await;
                break;
            }
        }
    }

    // R29-Phase-I: every exit path lands here, so the session
    // log always ends with a `lifecycle_shutdown` marker and the
    // sink is closed exactly once.
    inner.finalise_shutdown(exit_reason).await;
    inner.alive.store(false, Ordering::SeqCst);
}

/// One received operation, dispatched to its handler. The
/// `Shutdown` variant returns `true` — the loop breaks with
/// `Interrupted`; every other op returns `false` and the
/// loop continues.
///
/// A submitted operation may carry a per-turn provider selection.
/// It is parked on the inner state by the variants that lead to a
/// run, and consumed by the run they start; the variants that only
/// touch an existing run (`Cancel`, `Feedback`, `Shutdown`) leave it
/// alone, so a selection can never attach itself to an unrelated
/// later turn.
async fn handle_received_op(
    inner: &Arc<ControllerInner>,
    submitted: SubmittedOp,
    run_handle: &mut Option<tokio::task::JoinHandle<()>>,
) -> bool {
    let (op, provider) = submitted.into_parts();
    match op {
        SessionOp::Prompt { content, priority } => {
            park_provider(inner, provider);
            handle_prompt_op(inner, content, priority).await;
            start_run_if_idle(inner, run_handle).await;
        }
        SessionOp::PromptMulti {
            parts,
            agent_name,
            priority,
        } => {
            park_provider(inner, provider);
            handle_prompt_multi_op(inner, parts, agent_name, priority).await;
            start_run_if_idle(inner, run_handle).await;
        }
        SessionOp::Rerun {
            parts,
            agent_name,
            priority,
        } => {
            park_provider(inner, provider);
            handle_rerun_op(inner, parts, agent_name, priority).await;
            start_run_if_idle(inner, run_handle).await;
        }
        SessionOp::Feedback {
            message_id,
            thumbs_up,
        } => {
            handle_feedback_op(inner, message_id, thumbs_up).await;
        }
        SessionOp::Steer { content, priority } => {
            park_provider(inner, provider);
            handle_steer_op(inner, content, priority).await;
            start_run_if_idle(inner, run_handle).await;
        }
        SessionOp::Cancel { reason } => {
            handle_cancel_op(inner, reason).await;
        }
        SessionOp::Shutdown => {
            handle_shutdown_op(inner, run_handle).await;
            return true;
        }
    }
    false
}

/// Park the turn's provider selection for the next run to consume.
///
/// Mirrors how a multimodal op parks its `agent_name`: the selection
/// belongs to the turn, not to the session, so it lives on the inner
/// state only until [`ControllerInner::maybe_start_run`] takes it.
/// When several text prompts queue behind a running turn they share
/// one run, so the last selection parks last and wins — a selection
/// can never silently outlive the turn that made it.
fn park_provider(
    inner: &ControllerInner,
    provider: Option<Arc<dyn ModelProvider>>,
) {
    if let Some(provider) = provider {
        tracing::debug!(
            target: "synthia.session",
            session_id = %inner.session_id,
            provider = provider.name(),
            "op_rx: turn carries an explicit provider selection"
        );
        *inner.pending_provider.lock() = Some(provider);
    }
}

// --- `run_controller_loop` op handlers -----------------------------------
//
// One handler per `SessionOp` variant, plus the shared
// text-queue push (`Prompt` and `Steer` differ only in their
// log line) and the uniform post-op starter. The loop itself
// keeps only the `select!` shape and the run-handle /
// exit-reason bookkeeping.

/// `Prompt`: log, then push the text onto the session input
/// queue. The prompt itself is NOT persisted here — see the
/// comment inside.
async fn handle_prompt_op(
    inner: &ControllerInner,
    content: String,
    priority: u8,
) {
    tracing::info!(
        target: "synthia.session",
        session_id = %inner.session_id,
        op = "Prompt",
        priority,
        preview = truncate(&content, 40),
        "op_rx: received Prompt"
    );
    // The user prompt itself is NOT persisted to the sink
    // here — it is persisted at run completion (see the
    // run-task body). Persisting here would let the
    // about-to-start run read its own prompt back out of
    // `sink_history` and feed it to the agent a second time
    // (once via `history` and once via `content`), making
    // the LLM treat the prompt as a duplicate and discard
    // the conversation context. We persist at run completion
    // so the NEXT run sees this turn's prompt in
    // `sink_history` (multi-turn memory), while THIS run
    // only sees prior turns.
    push_text_input(inner, content).await;
}

/// `PromptMulti`: log, then park the multimodal payload on
/// the controller. `maybe_start_run` picks the parked
/// payload over any stale text in the InputQueue — the
/// multimodal prompt IS this turn, not an addition to the
/// queue.
async fn handle_prompt_multi_op(
    inner: &ControllerInner,
    parts: Vec<synthia::provider::ContentPart>,
    agent_name: Option<String>,
    priority: u8,
) {
    tracing::info!(
        target: "synthia.session",
        session_id = %inner.session_id,
        op = "PromptMulti",
        priority,
        part_count = parts.len(),
        agent_name = agent_name.as_deref().unwrap_or("-"),
        "op_rx: received PromptMulti"
    );
    *inner.pending_multimodal.lock() = Some(ParkedPrompt {
        parts,
        agent_name,
        rerun: false,
    });
}

/// `Rerun`: log, cancel any in-flight run, then park the
/// recovered parts with `rerun: true` (what the run task
/// reads to give the persisted prompt row replace
/// semantics — see `rerun_replace_op`). Cancelling first
/// guarantees the new turn starts against a clean state —
/// otherwise the rerun would queue behind the
/// still-running original turn.
async fn handle_rerun_op(
    inner: &ControllerInner,
    parts: Vec<synthia::provider::ContentPart>,
    agent_name: Option<String>,
    priority: u8,
) {
    tracing::info!(
        target: "synthia.session",
        session_id = %inner.session_id,
        op = "Rerun",
        priority,
        part_count = parts.len(),
        agent_name = agent_name.as_deref().unwrap_or("-"),
        "op_rx: received Rerun"
    );
    cancel_inflight_run(inner);
    *inner.pending_multimodal.lock() = Some(ParkedPrompt {
        parts,
        agent_name,
        rerun: true,
    });
}

/// `Feedback`: log, then append the feedback row as a JSONL
/// event so a future analytics endpoint can aggregate.
/// Failures are logged but do not abort the controller —
/// feedback is observational and never critical to the next
/// run.
async fn handle_feedback_op(
    inner: &ControllerInner,
    message_id: String,
    thumbs_up: bool,
) {
    tracing::info!(
        target: "synthia.session",
        session_id = %inner.session_id,
        op = "Feedback",
        message_id,
        thumbs_up,
        "op_rx: received Feedback; persisting to sink"
    );
    let payload = serde_json::json!({
        "kind": "feedback",
        "message_id": message_id,
        "thumbs_up": thumbs_up,
        "ts": inner.deps.lock().clock.now().to_rfc3339(),
    });
    if let Err(e) = inner.session_store.append(&payload).await {
        tracing::warn!(
            target: "synthia.session",
            session_id = %inner.session_id,
            error = %e,
            "Failed to persist feedback event"
        );
    }
}

/// `Steer`: log, then push the text onto the session input
/// queue (same channel as `Prompt`; the run loop drains it
/// between iterations).
async fn handle_steer_op(
    inner: &ControllerInner,
    content: String,
    priority: u8,
) {
    tracing::info!(
        target: "synthia.session",
        session_id = %inner.session_id,
        op = "Steer",
        priority,
        preview = truncate(&content, 40),
        "op_rx: received Steer"
    );
    push_text_input(inner, content).await;
}

/// Push `content` onto the session input queue. Shared by
/// `Prompt` and `Steer`; a push failure is logged and
/// dropped — the controller keeps serving.
async fn push_text_input(inner: &ControllerInner, content: String) {
    if let Err(e) = inner
        .queue
        .push(
            &inner.user_id,
            &inner.session_id,
            Value::String(content),
            Some(()),
        )
        .await
    {
        tracing::error!(
            target: "synthia.session",
            session_id = %inner.session_id,
            error = %e,
            "Failed to push input to session queue"
        );
    }
}

/// Fire the in-flight run's cancel token (without taking
/// it — the run task owns the slot until it exits). Shared
/// by `Rerun` (clean state before the new turn) and
/// `Cancel`.
fn cancel_inflight_run(inner: &ControllerInner) {
    if let Some(token) = inner
        .run_cancel
        .lock()
        .expect("run_cancel mutex poisoned")
        .as_ref()
    {
        token.cancel();
    }
}

/// Drop every queued text input so the controller does not
/// immediately restart the run after a cancellation. A
/// drain failure is logged and dropped — the controller
/// keeps serving.
async fn drain_pending_inputs(inner: &ControllerInner) {
    if let Err(e) = inner
        .queue
        .drain_pending(&inner.user_id, &inner.session_id)
        .await
    {
        tracing::error!(
            target: "synthia.session",
            session_id = %inner.session_id,
            error = %e,
            "Failed to drain pending inputs on cancel"
        );
    }
}

/// `Cancel`: fire the run's cancel token, drop any queued
/// inputs (so the controller does not immediately restart
/// the run), drop a parked multimodal payload (otherwise the
/// next run would silently re-send the same attachment the
/// user just tried to cancel), flip the state, and publish
/// the Cancelled snapshot (R11).
async fn handle_cancel_op(inner: &ControllerInner, reason: Option<String>) {
    tracing::info!(
        target: "synthia.session",
        session_id = %inner.session_id,
        op = "Cancel",
        reason = reason.as_deref().unwrap_or("-"),
        "op_rx: received Cancel; firing cancellation token"
    );
    cancel_inflight_run(inner);
    drain_pending_inputs(inner).await;
    *inner.pending_multimodal.lock() = None;
    publish_cancelled_snapshot(inner, &reason);
    if let Some(reason) = reason {
        tracing::info!(
            target: "synthia.session",
            session_id = %inner.session_id,
            reason,
            "Session run cancelled"
        );
    }
}

/// Flip the controller state to `Cancelled` and publish the
/// R11 `Cancelled` operation snapshot on the snapshot bus.
/// Shared shape with the run task's terminal snapshots.
fn publish_cancelled_snapshot(
    inner: &ControllerInner,
    reason: &Option<String>,
) {
    let mut state = inner.state.lock().expect("state mutex poisoned");
    *state = SessionState::Cancelled;
    let _seq =
        inner
            .snapshot_bus
            .publish(synthia::session::OperationSnapshot::new(
                inner.session_id.clone(),
                "default".to_string(),
                synthia::session::OperationState::Cancelled {
                    reason: reason
                        .clone()
                        .unwrap_or_else(|| "cancelled".into()),
                },
                0,
                inner.deps.lock().default_max_iterations,
                synthia::provider::TokenUsage::default(),
            ));
}

/// `Shutdown`: take + fire the cancel token, then bound the
/// in-flight run's join with a 3 s deadline (R29-Phase-I) so
/// a hung run task cannot keep the shutdown — and the
/// process — alive. The caller breaks out of the loop after
/// this returns.
async fn handle_shutdown_op(
    inner: &ControllerInner,
    run_handle: &mut Option<tokio::task::JoinHandle<()>>,
) {
    tracing::info!(
        target: "synthia.session",
        session_id = %inner.session_id,
        op = "Shutdown",
        "op_rx: received Shutdown; breaking controller loop"
    );
    if let Some(token) = inner
        .run_cancel
        .lock()
        .expect("run_cancel mutex poisoned")
        .take()
    {
        token.cancel();
    }
    if let Some(h) = run_handle.take()
        && tokio::time::timeout(Duration::from_secs(3), h)
            .await
            .is_err()
    {
        tracing::warn!(
            target: "synthia.session",
            session_id = %inner.session_id,
            "shutdown: in-flight run did not stop within 3s; abandoning it"
        );
    }
}

/// The uniform post-op starter: when no run is in flight,
/// hand `maybe_start_run` the chance to consume whatever
/// input slots hold (the text queue and/or the parked
/// multimodal payload).
async fn start_run_if_idle(
    inner: &Arc<ControllerInner>,
    run_handle: &mut Option<tokio::task::JoinHandle<()>>,
) {
    if run_handle.is_none()
        && let Some(h) = inner.maybe_start_run().await
    {
        *run_handle = Some(h);
    }
}

/// The `select!` run-completion arm body: clear the handle,
/// surface a panicked run, and — if more inputs arrived
/// while the run was active — start the next run
/// immediately. The gate must be `maybe_start_run` itself,
/// which checks BOTH input slots (the text queue and the
/// parked multimodal payload) and returns `None` when
/// neither holds anything. Testing only the text queue here
/// silently dropped a `Rerun`/`PromptMulti` that arrived
/// mid-run: those park their lossless parts in
/// `pending_multimodal`, leaving the text queue empty, so no
/// restart happened and the parts sat parked until some
/// later op's start consumed them — the agent would then
/// answer the stale prompt instead of the new one.
async fn on_run_completed(
    inner: &Arc<ControllerInner>,
    result: Result<(), tokio::task::JoinError>,
    run_handle: &mut Option<tokio::task::JoinHandle<()>>,
) {
    *run_handle = None;
    if let Err(e) = result {
        tracing::error!(
            session_id = %inner.session_id,
            error = %e,
            "Session run task panicked"
        );
    }
    if let Some(h) = inner.maybe_start_run().await {
        *run_handle = Some(h);
    }
}

// --- `run_controller_loop` select! guard futures + exit arms -------------

/// The `select!` guard future for the run-completion arm:
/// await the in-flight run's join handle, or park forever
/// when none exists (the arm's precondition keeps that
/// branch unpollled anyway; the never-resolving future is
/// just the type-level filler `select!` needs).
async fn wait_for_run(
    run_handle: &mut Option<tokio::task::JoinHandle<()>>,
) -> Result<(), tokio::task::JoinError> {
    match run_handle {
        Some(h) => h.await,
        None => pending::<Result<(), tokio::task::JoinError>>().await,
    }
}

/// The `select!` guard future for the idle-timeout arm.
async fn wait_idle_timeout(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => pending::<()>().await,
    }
}

/// Await a taken run handle to completion, ignoring its
/// outcome — used on the exit paths where the controller is
/// shutting down either way.
async fn join_run(handle: Option<tokio::task::JoinHandle<()>>) {
    if let Some(h) = handle {
        let _ = h.await;
    }
}

/// The idle-timeout arm body: log, cancel any in-flight run,
/// and await it. The caller breaks out of the loop after
/// this returns.
async fn handle_idle_timeout(
    inner: &Arc<ControllerInner>,
    run_handle: &mut Option<tokio::task::JoinHandle<()>>,
) {
    tracing::info!(
        session_id = %inner.session_id,
        "Session controller idle timeout reached; shutting down"
    );
    if let Some(token) = inner
        .run_cancel
        .lock()
        .expect("run_cancel mutex poisoned")
        .take()
    {
        token.cancel();
    }
    join_run(run_handle.take()).await;
}

async fn idle_deadline(
    inner: &ControllerInner,
    last_activity: &tokio::time::Instant,
    run_active: bool,
) -> Option<tokio::time::Instant> {
    if run_active
        || inner.broadcaster.subscriber_count() > 0
        || inner
            .queue
            .has_pending(&inner.user_id, &inner.session_id)
            .await
    {
        None
    } else {
        Some(*last_activity + inner.idle_timeout)
    }
}
