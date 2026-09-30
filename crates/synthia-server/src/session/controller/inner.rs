//! The controller's inner state: shared between the session handle,
//! the dispatch loop and the run task. Lifecycle shutdown, the
//! state gates, and run-config assembly live on it.

use std::{
    sync::{
        Arc,
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use synthia::{
    core::Clock,
    harness::{AgentRunConfig, CompactionEmitters},
    provider::traits::ModelProvider,
    session::{
        CompactionCheckpoint,
        SessionSink,
        manager::InputQueue as SessionInputQueue,
    },
    tool::registry::ToolRegistry,
};
use tokio_util::sync::CancellationToken;

use super::{
    deps::RunDependencies,
    ops::SessionState,
    run_stream::RunStreamFactory,
};
use crate::{event_stream::EventBroadcaster, state::UsageMetrics};

/// One parked turn waiting for the next run: the lossless
/// multimodal parts the text queue cannot carry, plus whether
/// the turn is a fresh prompt (`PromptMulti`) or a rerun of the
/// previous turn (`Rerun`).
///
/// A rerun's persisted prompt row must shadow the turn it
/// replaces (see `rerun_replace_op`); every other consumer of
/// the slot treats both kinds identically.
pub(super) struct ParkedPrompt {
    pub(super) parts: Vec<synthia::provider::ContentPart>,
    pub(super) agent_name: Option<String>,
    /// `true` when parked by `SessionOp::Rerun`.
    pub(super) rerun: bool,
}

pub(super) struct ControllerInner {
    pub(super) session_id: String,
    pub(super) user_id: String,
    pub(super) state: Arc<Mutex<SessionState>>,
    pub(super) queue: SessionInputQueue,
    pub(super) session_store: Arc<dyn SessionSink>,
    pub(super) broadcaster: EventBroadcaster,
    pub(super) deps: parking_lot::Mutex<RunDependencies>,
    /// Process-wide usage counters this controller records into —
    /// the shared `Arc` from its [`RunDependencies`], so the totals
    /// survive the session closing. Written only by
    /// [`ControllerInner::persist_and_broadcast`], the single funnel
    /// every agent event passes through.
    pub(super) usage: Arc<UsageMetrics>,
    /// Sub-agent session router: delegated child events are handed
    /// here (their own sessions) instead of joining this session's
    /// log and broadcast. `None` drops them.
    pub(super) subagent_router:
        Option<Arc<crate::session::subagent_router::SubagentRouter>>,
    /// `(provider, model, tools_hash)` of the last typed
    /// `request_header` event written to the sink. The next
    /// run compares against this and re-emits the header only
    /// on `initial` (first run) or `change` (config drift) —
    pub(super) last_request_header:
        parking_lot::Mutex<Option<(String, String, u64)>>,
    pub(super) idle_timeout: Duration,
    pub(super) run_cancel: Mutex<Option<CancellationToken>>,
    pub(super) run_factory: Arc<dyn RunStreamFactory>,
    pub(super) alive: Arc<AtomicBool>,
    /// R11: read-only operation-snapshot bus. Publishes an
    /// [`synthia::session::OperationSnapshot`] at every run-state
    /// transition (Running / Idle / Cancelled) so HTTP `/status`,
    /// TUI, and replay consumers can observe progress without
    /// touching the run task internals.
    pub(super) snapshot_bus: synthia::session::SnapshotBus,
    /// [`SessionInputQueue`] cannot represent losslessly.
    /// Populated by the `PromptMulti` / `Rerun` op paths and
    /// consumed by the next `maybe_start_run`. Per-run (cleared
    /// by `maybe_start_run` before the agent is invoked) so a
    /// stuck or cancelled run does not re-send an old
    /// attachment on the next dispatch.
    pub(super) pending_multimodal: parking_lot::Mutex<Option<ParkedPrompt>>,
    /// The provider selection the next run samples with, parked by
    /// [`super::dispatch`] when an operation carries one (see
    /// [`PinnedProvider`](super::PinnedProvider)).
    ///
    /// `None` — the overwhelmingly common case — means the run uses
    /// the provider on [`RunDependencies`]. Like the parked
    /// multimodal payload this is per-run state: `maybe_start_run`
    /// takes it, so a selection cannot outlive the turn that made it.
    pub(super) pending_provider:
        parking_lot::Mutex<Option<Arc<dyn ModelProvider>>>,
    /// R29-Phase-I: one-shot guard for the terminal
    /// `LifecycleShutdown` emission. Every loop exit path calls
    /// `finalise_shutdown`; the first call wins so a
    /// shutdown-then-idle race cannot double-append.
    pub(super) shutdown_finalised: AtomicBool,
}

impl ControllerInner {
    /// Append the terminal `LifecycleShutdown` session event and
    /// close the session sink. Idempotent.
    ///
    /// Called from every exit path of the controller loop so a
    /// session log never ends without a shutdown marker — the
    /// counterpart of the `CompactionStart`/`CompactionEnd`
    /// pairing for the whole session.
    pub(super) async fn finalise_shutdown(
        &self,
        reason: synthia::session::SessionEndReason,
    ) {
        if self.shutdown_finalised.swap(true, Ordering::SeqCst) {
            return;
        }
        let event = synthia::session::SessionEvent::LifecycleShutdown {
            seq: 0,
            ts: self.deps.lock().clock.now().to_rfc3339(),
            data: serde_json::json!({ "reason": reason }),
        };
        self.append_lifecycle_event(&event).await;
        self.close_sink(reason).await;
    }

    /// Serialize + append one lifecycle terminal event,
    /// logging (not propagating) either failure — the
    /// shutdown path must run to the sink close regardless.
    async fn append_lifecycle_event(
        &self,
        event: &synthia::session::SessionEvent,
    ) {
        let value = match serde_json::to_value(event) {
            Ok(value) => value,
            Err(e) => {
                tracing::warn!(
                    target: "synthia.session",
                    session_id = %self.session_id,
                    error = %e,
                    "Failed to serialise lifecycle_shutdown event"
                );
                return;
            }
        };
        if let Err(e) = self.session_store.append(&value).await {
            tracing::warn!(
                target: "synthia.session",
                session_id = %self.session_id,
                error = %e,
                "Failed to append lifecycle_shutdown event"
            );
        }
    }

    /// Close the session sink. A close failure is logged but
    /// does not abort the shutdown — the sink remains
    /// reachable on the next start.
    async fn close_sink(&self, reason: synthia::session::SessionEndReason) {
        if let Err(e) = self.session_store.close(reason).await {
            tracing::warn!(
                target: "synthia.session",
                session_id = %self.session_id,
                error = %e,
                "Failed to close session sink"
            );
        }
    }

    /// The state-gate check `maybe_start_run` runs first: the
    /// controller only starts a new run when it's Idle or
    /// Cancelled (the latter because a cancellation marks the
    /// state, then maybe_start_run wakes the controller back
    /// up — see the Cancel op handler).
    pub(super) fn is_startable(&self) -> bool {
        let state = self.state.lock().expect("state mutex poisoned");
        if *state != SessionState::Idle && *state != SessionState::Cancelled {
            tracing::trace!(
                target: "synthia.session",
                session_id = %self.session_id,
                state = ?*state,
                "maybe_start_run: skipped (not Idle/Cancelled)"
            );
            return false;
        }
        true
    }

    /// Transition the controller to `Running`, log the run's
    /// shape, publish the R11 Running snapshot (so `/status`
    /// subscribers observe the transition immediately), and
    /// install a fresh cancel token for the run about to
    /// start.
    pub(super) async fn mark_running(&self, multimodal_active: bool) {
        {
            let mut state = self.state.lock().expect("state mutex poisoned");
            *state = SessionState::Running;
        }
        let pending_count = self
            .queue
            .has_pending(&self.user_id, &self.session_id)
            .await as usize;
        tracing::info!(
            target: "synthia.session",
            session_id = %self.session_id,
            pending_count,
            multimodal = multimodal_active,
            subscribers = self.broadcaster.subscriber_count(),
        );
        let agent_name = self
            .deps
            .lock()
            .default_agent_name
            .as_ref()
            .and_then(|l| l.read().clone())
            .unwrap_or_else(|| "default".to_string());
        let max_iters = self.deps.lock().default_max_iterations;
        let _seq = self.snapshot_bus.publish(
            synthia::session::OperationSnapshot::started(
                self.session_id.clone(),
                agent_name,
                max_iters,
            ),
        );

        let cancel_token = CancellationToken::new();
        *self.run_cancel.lock().expect("run_cancel mutex poisoned") =
            Some(cancel_token.clone());
    }

    /// Build an [`AgentRunConfig`] for the next run. Lets the
    /// caller pin the agent name for THIS run only; the
    /// configured default is still consulted as a fallback (and
    /// the first registered agent as a last resort) — the
    /// `explicit` argument just takes the top of the
    /// `explicit > configured default > first registered` ladder.
    ///
    /// `provider` pins the model backing for THIS run only: the
    /// turn's `model` selection, already resolved to a handle by
    /// [`AppState::resolve_provider`](crate::state::AppState::resolve_provider).
    /// `None` keeps the deployment's configured provider. The pinned
    /// handle also composes the context manager, so a summarising
    /// strategy summarises with the model that actually runs — and
    /// the durable `request_header` the run stamps names it, because
    /// the run task reads the header off this config rather than off
    /// the dependencies.
    pub(super) fn build_run_config_with_explicit_agent(
        &self,
        explicit: Option<&str>,
        provider: Option<Arc<dyn ModelProvider>>,
        checkpoint: Option<&Arc<CompactionCheckpoint>>,
    ) -> AgentRunConfig {
        let deps = self.deps.lock();
        let provider = provider.unwrap_or_else(|| Arc::clone(&deps.provider));
        let tool_registry = deps
            .tool_registry
            .try_read()
            .map(|r| Arc::new((*r).clone()))
            .unwrap_or_else(|_| Arc::new(ToolRegistry::new()));

        // Sync agent-name resolution via the
        // `AppState::resolve_agent_name` helper. The dispatch
        // path is unified: every request — chat
        // scheduler — flows through `SessionController` and
        // shares this single ladder
        // (`explicit > configured default > first registered`).
        let default_name = deps
            .default_agent_name
            .as_ref()
            .and_then(|m| m.read().clone());
        let agent_name = deps.agent_registry.as_ref().and_then(|reg| {
            crate::state::AppState::resolve_agent_name(
                reg,
                default_name.as_deref(),
                explicit,
            )
        });

        AgentRunConfig {
            provider: Arc::clone(&provider),
            tool_registry,
            workspace_root: deps.workspace_root.clone(),
            system_prompt: deps.system_prompt.clone(),
            prompt_context: deps.prompt_context.clone(),
            agent_resolver: deps.agent_registry.as_ref().map(|reg| {
                let reg = Arc::clone(reg);
                Arc::new(move |name: String| {
                    reg.resolve_sync(&name).map(|a| a.descriptor().clone())
                })
                    as Arc<
                        dyn Fn(
                                String,
                            )
                                -> Option<synthia::core::agent::AgentDescriptor>
                            + Send
                            + Sync,
                    >
            }),
            agent_name,
            max_iterations: Some(deps.default_max_iterations),
            steering: Arc::clone(&deps.steering),
            // Pass the registry through so callers (and future
            // fan-out strategies) can resolve peer agents.
            // Cheap to clone (Arc).
            agent_registry: deps.agent_registry.clone(),
            // R6-B: the typed-event sink is created per-run by
            // `maybe_start_run` (channel + drain task); this
            // default builder leaves it unset.
            typed_event_sink: None,
            // R16: the deployment's compaction capability composes
            // into one context manager. `None` keeps the loop's
            // default manager. R34: the
            // compaction checkpoint's emitters ride along, so a
            // splice is recorded durably (or, when its provenance
            // cannot be proven, recorded log-only).
            context_manager: deps.compose_context_manager_with_emitters(
                Arc::clone(&provider),
                checkpoint.map(|checkpoint| {
                    CompactionEmitters::new(
                        checkpoint.record_callback(),
                        checkpoint.lifecycle_callback(),
                    )
                }),
            ),
            // R34: the deployment's `[tools]` surface policy (groups
            // + `max_visible`). The registry already carries the
            // boot-applied exposures; the cap is request-side.
            tool_surface: deps.tool_surface.clone(),
            // R50: the deployment's configured reasoning loop.
            strategy: deps.strategy.clone(),
            // R58: the agent's own allow/deny list.
            tool_restriction: deps.tool_restriction.clone(),
        }
    }
}
