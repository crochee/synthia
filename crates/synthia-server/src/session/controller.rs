//! Per-session controller that serializes prompt/steer/cancel operations
//! and ensures at most one `Agent::run` per session.

use std::{
    future::pending,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc,
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context as _, Result};
use futures::{Stream, StreamExt};
use serde_json::Value;
use synthia_agent::{
    Agent,
    AgentEvent,
    AgentInput,
    AgentRunConfig,
    PromptContext,
    ReActAgent,
};
use synthia_provider::{
    Content,
    ContentPart,
    Message,
    Role,
    TextContent,
    traits::ModelProvider,
};
use synthia_session::{
    SessionError,
    SessionSink,
    manager::InputQueue as SessionInputQueue,
};
use synthia_tool::registry::ToolRegistry;
use tokio::sync::{RwLock, mpsc};
use tokio_util::sync::CancellationToken;

use crate::event_stream::EventBroadcaster;

/// Default idle timeout before the controller shuts itself down when
/// no run is active and there are no streaming subscribers.
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Operations that can be submitted to a [`SessionController`].
#[derive(Debug, Clone)]
pub enum SessionOp {
    Prompt { content: String, priority: u8 },
    Steer { content: String, priority: u8 },
    Cancel { reason: Option<String> },
    Shutdown,
}

/// Lifecycle state of a session controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Idle,
    Running,
    Cancelled,
}

/// Factory abstraction so the controller can be unit-tested without
/// starting a real agent run.
pub trait RunStreamFactory: Send + Sync + 'static {
    fn run_stream(
        &self,
        config: AgentRunConfig,
        input: AgentInput,
        cancel: Arc<CancellationToken>,
    ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>>;
}

/// Production implementation that delegates to
/// [`synthia_agent::Agent`].
#[derive(Debug, Clone, Default)]
pub struct AgentRunStreamFactory;

impl RunStreamFactory for AgentRunStreamFactory {
    fn run_stream(
        &self,
        config: AgentRunConfig,
        input: AgentInput,
        cancel: Arc<CancellationToken>,
    ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>> {
        // Resolve the descriptor through the dispatcher when
        // the caller supplied an `agent_resolver` + `agent_name`.
        // The legacy path (no resolver) keeps using the
        // `system_prompt` as the base instructions.
        let provider = Arc::clone(&config.provider);
        let tool_registry = Arc::clone(&config.tool_registry);
        let workspace_root = config.workspace_root.clone();
        let system_prompt = config.system_prompt.clone();
        let prompt_context = config.prompt_context.clone();
        let resolver = config.agent_resolver.clone();
        let resolved_descriptor = if let (Some(r), Some(n)) =
            (resolver.as_ref(), config.agent_name.as_ref())
        {
            match r(n.clone()) {
                Some(d) => Some(d),
                None => {
                    tracing::warn!(
                        agent_name = %n,
                        "agent_resolver returned None; falling back to default descriptor"
                    );
                    None
                }
            }
        } else {
            None
        };
        // Multi-agent orchestration: panel / role fields were
        // removed from `AgentDescriptor`, so the run factory
        // always builds a single `ReActAgent`. A caller wanting
        // multi-agent orchestration composes its own fan-out
        // on top of the returned event stream — there is no
        // built-in panel coordinator in the agent runtime.

        // Build the agent with the assembled prompt context
        // (skills + peer agents + tool manifest) so the system
        // prompt that reaches the LLM carries the full
        // industry-aligned manifest, not just the base
        // instructions.
        let agent = match resolved_descriptor {
            Some(descriptor) => Arc::new(ReActAgent::with_descriptor(
                provider,
                tool_registry,
                workspace_root,
                descriptor,
                prompt_context,
            )),
            None => Arc::new(ReActAgent::with_prompt_context(
                provider,
                tool_registry,
                workspace_root,
                system_prompt,
                prompt_context,
            )),
        };
        // `Agent::run` is `async` (via `#[async_trait]`) so we
        // bridge the future into a stream by awaiting it once
        // and yielding each event. The agent surfaces errors
        // through `AgentEvent::System(SessionEnded{Error})`,
        // so the returned stream carries no `Result`.
        Box::pin(async_stream::stream! {
            let mut inner = agent.run(input, cancel).await;
            while let Some(item) = inner.next().await {
                yield item;
            }
        })
    }
}

/// Minimal dependencies required to build an [`AgentRunConfig`] for
/// the session.
#[derive(Clone)]
pub struct RunDependencies {
    pub provider: Arc<dyn ModelProvider>,
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
    /// Working directory handed to built-in tools via
    /// [`AgentRunConfig::workspace_root`]. Replaces the
    /// previous hard-coded `/tmp` so `read_file` / `shell`
    /// operate inside the user's project root, not the
    /// system temp dir.
    pub workspace_root: PathBuf,
    /// System prompt injected as the first message of every
    /// conversation. The ReAct loop builds its own system
    /// prompt from the descriptor via
    /// [`synthia_agent::prompt::PromptContext::assemble`]; this
    /// field is the legacy/default fallback for callers that
    /// do not supply an explicit descriptor.
    pub system_prompt: String,
    /// Prompt-context manifest (skills + peer agents + tool
    /// definitions). Empty by default; populated by the server
    /// from the workspace's `.agents/skills/` directory and the
    /// registered [`AgentRegistry`] at startup.
    ///
    /// Stored as `Arc<PromptContext>` so cloning the
    /// [`RunDependencies`] (which the [`crate::state::AppState`]
    /// does on every session creation) bumps a refcount instead
    /// of deep-cloning the skills + peer-agents lists. The
    /// manifest is read-only after boot, so this is safe across
    /// concurrent dispatches.
    pub prompt_context: Arc<synthia_agent::prompt::PromptContext>,
    /// Multi-agent registry. Held by reference so the run
    /// factory can resolve the configured agent synchronously
    /// inside `build_run_config` without going through an
    /// async dispatch boundary.
    pub agent_registry: Option<Arc<synthia_agent::AgentRegistry>>,
    /// Configured default agent name (parking_lot-backed, so
    /// the factory can read it synchronously).
    pub default_agent_name: Option<Arc<parking_lot::RwLock<Option<String>>>>,
}

impl RunDependencies {
    pub fn new(
        provider: Arc<dyn ModelProvider>,
        tool_registry: Arc<RwLock<ToolRegistry>>,
        workspace_root: PathBuf,
        system_prompt: String,
    ) -> Self {
        Self {
            provider,
            tool_registry,
            workspace_root,
            system_prompt,
            prompt_context: Arc::new(PromptContext::default()),
            agent_registry: None,
            default_agent_name: None,
        }
    }

    /// Attach a populated prompt context so the agent's system
    /// prompt carries the skill/agent/tool manifest. The caller
    /// is expected to wrap the manifest in an `Arc` itself so
    /// multiple session controllers can share the same backing
    /// allocation.
    pub fn with_prompt_context(mut self, ctx: Arc<PromptContext>) -> Self {
        self.prompt_context = ctx;
        self
    }

    /// Wire the multi-agent registry + configured default so
    /// the run factory can resolve the configured agent
    /// synchronously inside `build_run_config`.
    pub fn with_agent_registry(
        mut self,
        registry: Arc<synthia_agent::AgentRegistry>,
        default_agent_name: Arc<parking_lot::RwLock<Option<String>>>,
    ) -> Self {
        self.agent_registry = Some(registry);
        self.default_agent_name = Some(default_agent_name);
        self
    }
}

/// Shared handle to a running session controller.
pub struct SessionController {
    session_id: String,
    user_id: String,
    state: Arc<Mutex<SessionState>>,
    op_tx: mpsc::Sender<SessionOp>,
    broadcaster: EventBroadcaster,
    alive: Arc<AtomicBool>,
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

        let controller = Arc::new(Self {
            session_id: session_id.clone(),
            user_id: user_id.clone(),
            state: state.clone(),
            op_tx,
            broadcaster: broadcaster.clone(),
            alive: alive.clone(),
        });

        let inner = Arc::new(ControllerInner {
            session_id: controller.session_id.clone(),
            user_id: controller.user_id.clone(),
            state,
            queue,
            session_store,
            broadcaster,
            deps: parking_lot::Mutex::new(deps),
            idle_timeout,
            run_cancel: Mutex::new(None),
            run_factory,
            alive,
        });

        tokio::spawn(run_controller_loop(inner, op_rx));

        controller
    }

    /// Submit an operation to the serialized controller loop.
    pub async fn submit(&self, op: SessionOp) -> Result<()> {
        self.op_tx
            .send(op)
            .await
            .context("session controller is shut down")?;
        Ok(())
    }

    /// Convenience helper to request cancellation of the current run.
    pub async fn cancel(&self) -> Result<()> {
        self.submit(SessionOp::Cancel { reason: None }).await
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
}

struct ControllerInner {
    session_id: String,
    user_id: String,
    state: Arc<Mutex<SessionState>>,
    queue: SessionInputQueue,
    session_store: Arc<dyn SessionSink>,
    broadcaster: EventBroadcaster,
    deps: parking_lot::Mutex<RunDependencies>,
    idle_timeout: Duration,
    run_cancel: Mutex<Option<CancellationToken>>,
    run_factory: Arc<dyn RunStreamFactory>,
    alive: Arc<AtomicBool>,
}

impl ControllerInner {
    /// Start a new agent run if and only if the controller is idle and
    /// there are pending inputs in the queue.
    async fn maybe_start_run(
        self: &Arc<Self>,
    ) -> Option<tokio::task::JoinHandle<()>> {
        {
            let state = self.state.lock().expect("state mutex poisoned");
            if *state != SessionState::Idle && *state != SessionState::Cancelled
            {
                tracing::trace!(
                    target: "synthia.session",
                    session_id = %self.session_id,
                    state = ?*state,
                    "maybe_start_run: skipped (not Idle/Cancelled)"
                );
                return None;
            }
        }

        if !self
            .queue
            .has_pending(&self.user_id, &self.session_id)
            .await
        {
            tracing::trace!(
                target: "synthia.session",
                session_id = %self.session_id,
                "maybe_start_run: skipped (no pending inputs)"
            );
            return None;
        }

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
            subscribers = self.broadcaster.subscriber_count(),
            "maybe_start_run: state Idle -> Running; spawning agent run"
        );

        let cancel_token = CancellationToken::new();
        *self.run_cancel.lock().expect("run_cancel mutex poisoned") =
            Some(cancel_token.clone());

        let config = self.build_run_config();
        let factory = Arc::clone(&self.run_factory);
        let inner = Arc::clone(self);

        Some(tokio::spawn(async move {
            // Drain the queued prompts so the agent sees the real
            // user input. Without this, the run is started with the
            // empty `config.input` placeholder and every LLM call is
            // sent an empty user message.
            let pending = match inner
                .queue
                .drain_pending(&inner.user_id, &inner.session_id)
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!(
                        target: "synthia.session",
                        session_id = %inner.session_id,
                        error = %e,
                        "Failed to drain session input queue"
                    );
                    Vec::new()
                }
            };
            tracing::debug!(
                target: "synthia.session",
                session_id = %inner.session_id,
                drained_count = pending.len(),
                "Agent run task: drained input queue"
            );
            let (pending_history, prompt) = pending.into_iter().fold(
                (Vec::new(), String::new()),
                |(mut hist, mut last), entry| {
                    if last.is_empty() {
                        last = entry.content;
                    } else {
                        hist.push(Message {
                            role: Role::User,
                            content: Content::Single(ContentPart::Text(
                                TextContent {
                                    text: last,
                                    cache_control: None,
                                },
                            )),
                            tool_call_id: None,
                            name: None,
                            ..Default::default()
                        });
                        last = entry.content;
                    }
                    (hist, last)
                },
            );
            // Reconstruct prior-turn assistant / tool messages from
            // the durable events the previous runs persisted to the
            // session sink. Without this, every run starts with an
            // empty history and the LLM loses all multi-turn
            // memory. The sink is the only durable source of truth
            // here — `InputQueue` is per-run and ephemeral.
            let sink_history = match inner.session_store.read().await {
                Ok(events) => events_to_history(&events),
                Err(e) => {
                    tracing::error!(
                        target: "synthia.session",
                        session_id = %inner.session_id,
                        error = %e,
                        "Failed to read session sink; starting run with empty history"
                    );
                    Vec::new()
                }
            };
            tracing::debug!(
                target: "synthia.session",
                session_id = %inner.session_id,
                sink_history_len = sink_history.len(),
                pending_history_len = pending_history.len(),
                "Agent run task: composed history from sink + pending"
            );
            // Persist this run's drained user prompts to the sink so
            // the NEXT run sees them in `sink_history`. We do this
            // AFTER reading the sink (so this run doesn't echo its
            // own prompt back through `sink_history`) and BEFORE
            // invoking the agent (so a fast agent doesn't emit
            // assistant events before the user prompts are
            // recorded — `events_to_history` walks the JSONL in
            // append order, so user prompts must precede the
            // assistant turns they elicited).
            for msg in &pending_history {
                let text = match &msg.content {
                    Content::Single(ContentPart::Text(t)) => &t.text,
                    _ => continue,
                };
                if let Err(e) = inner
                    .session_store
                    .append(&serde_json::json!({
                        "type": "UserInput",
                        "data": { "text": text },
                    }))
                    .await
                {
                    tracing::error!(
                        target: "synthia.session",
                        session_id = %inner.session_id,
                        error = %e,
                        "Failed to persist drained user prompt to session sink"
                    );
                }
            }
            if !prompt.is_empty()
                && let Err(e) = inner
                    .session_store
                    .append(&serde_json::json!({
                        "type": "UserInput",
                        "data": { "text": &prompt },
                    }))
                    .await
            {
                tracing::error!(
                    target: "synthia.session",
                    session_id = %inner.session_id,
                    error = %e,
                    "Failed to persist current-turn user prompt to session sink"
                );
            }
            let mut history = sink_history;
            history.extend(pending_history);
            let input = if prompt.is_empty() {
                AgentInput::text("")
            } else if history.is_empty() {
                AgentInput::text(prompt)
            } else {
                AgentInput::history(history, prompt)
            };

            tracing::info!(
                target: "synthia.session",
                session_id = %inner.session_id,
                "Agent run task: invoking factory.run_stream"
            );
            let cancel = Arc::new(
                inner
                    .run_cancel
                    .lock()
                    .expect("run_cancel mutex poisoned")
                    .clone()
                    .expect("run_cancel token must be set before run starts"),
            );
            let mut stream = factory.run_stream(config, input, cancel);
            tracing::info!(
                target: "synthia.session",
                session_id = %inner.session_id,
                "Agent run task: factory returned; draining events"
            );
            let mut event_count = 0usize;
            while let Some(event) = stream.next().await {
                event_count += 1;
                if let Err(e) = inner.persist_and_broadcast(&event).await {
                    tracing::error!(
                        target: "synthia.session",
                        session_id = %inner.session_id,
                        event_kind = event.kind(),
                        error = %e,
                        "Failed to persist or broadcast event"
                    );
                }
            }
            tracing::info!(
                target: "synthia.session",
                session_id = %inner.session_id,
                event_count,
                "Agent run task: factory stream ended"
            );

            let mut state = inner.state.lock().expect("state mutex poisoned");
            if *state == SessionState::Running {
                *state = SessionState::Idle;
            }
            tracing::info!(
                target: "synthia.session",
                session_id = %inner.session_id,
                "Agent run task: drained; state Running -> Idle"
            );
        }))
    }

    fn build_run_config(&self) -> AgentRunConfig {
        let deps = self.deps.lock();
        let tool_registry = deps
            .tool_registry
            .try_read()
            .map(|r| Arc::new((*r).clone()))
            .unwrap_or_else(|_| Arc::new(ToolRegistry::new()));

        // Sync agent-name resolution via the
        // `AppState::resolve_agent_name` helper. The dispatch
        // path is unified: every request — chat, A2A,
        // scheduler — flows through `SessionController` and
        // shares this single ladder
        // (`configured default > first registered`).
        let default_name = deps
            .default_agent_name
            .as_ref()
            .and_then(|m| m.read().clone());
        let agent_name = deps.agent_registry.as_ref().and_then(|reg| {
            crate::state::AppState::resolve_agent_name(
                reg,
                default_name.as_deref(),
                None,
            )
        });

        AgentRunConfig {
            provider: Arc::clone(&deps.provider),
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
                        dyn Fn(String) -> Option<synthia_agent::AgentDescriptor>
                            + Send
                            + Sync,
                    >
            }),
            agent_name,
            // Pass the registry through so callers (and future
            // fan-out strategies) can resolve peer agents.
            // Cheap to clone (Arc).
            agent_registry: deps.agent_registry.clone(),
        }
    }

    async fn persist_and_broadcast(&self, event: &AgentEvent) -> Result<()> {
        let outer_kind = event.kind();
        let system_kind = match event {
            AgentEvent::System(sys) => Some(sys.kind()),
            _ => None,
        };
        // Single serialization pass. `serialized_size()` is a
        // cheap O(payload) measurement; we previously ran
        // `to_value(...).to_string().len()` (two passes) and then
        // re-serialized inside `EventBroadcaster::send` (three
        // passes total). With `to_vec` we keep ownership of the
        // bytes for the disk append AND recover the size for logs
        // in one shot.
        let payload_bytes = serde_json::to_vec(event)
            .context("failed to serialize agent event")?;
        let byte_size = payload_bytes.len();
        let event_type = event.kind();

        tracing::debug!(
            target: "synthia.session",
            session_id = %self.session_id,
            event_kind = outer_kind,
            system_kind = system_kind.unwrap_or("-"),
            event_type,
            payload_bytes = byte_size,
            subscribers = self.broadcaster.subscriber_count(),
            "persist_and_broadcast: entering"
        );

        // After the panel/session refactor, persistence is
        // owned by the `SessionSink` directly. The previous
        // code routed through `event_store().append_bytes()`
        // with an `EventSource::Agent` tag; the sink is
        // shape-agnostic (it stores opaque `serde_json::Value`
        // records), so we serialize the event once and
        // append.
        let event_value: serde_json::Value =
            serde_json::from_slice(&payload_bytes)
                .context("failed to decode event payload for sink append")?;
        // Ephemeral events (token deltas, warnings, reasoning
        // chunks) are streamed live but not persisted: a cold
        // start or replay rebuilds the agent's state from the
        // durable slices (model text, tool calls, tool
        // results, session-ended), so dropping the deltas
        // keeps the JSONL compact without losing anything the
        // model can't reconstruct.
        if event.is_durable() {
            self.session_store.append(&event_value).await.map_err(
                |e: SessionError| {
                    anyhow::anyhow!("failed to append event to sink: {e}")
                },
            )?;
        }
        tracing::trace!(
            target: "synthia.session",
            session_id = %self.session_id,
            event_kind = outer_kind,
            system_kind = system_kind.unwrap_or("-"),
            "persist_and_broadcast: appended to event store"
        );

        if let Err(e) = self.broadcaster.send(event.clone()) {
            tracing::debug!(
                target: "synthia.session",
                session_id = %self.session_id,
                event_kind = outer_kind,
                system_kind = system_kind.unwrap_or("-"),
                error = %e,
                "No subscribers to broadcast event to"
            );
        }

        tracing::debug!(
            target: "synthia.session",
            session_id = %self.session_id,
            event_kind = outer_kind,
            system_kind = system_kind.unwrap_or("-"),
            "persist_and_broadcast: done"
        );

        Ok(())
    }
}

async fn run_controller_loop(
    inner: Arc<ControllerInner>,
    mut op_rx: mpsc::Receiver<SessionOp>,
) {
    let mut last_activity = tokio::time::Instant::now();
    let mut run_handle: Option<tokio::task::JoinHandle<()>> = None;

    loop {
        let idle_deadline =
            idle_deadline(&inner, &last_activity, run_handle.is_some()).await;

        tokio::select! {
            biased;

            Some(op) = op_rx.recv() => {
                last_activity = tokio::time::Instant::now();
                match op {
                    SessionOp::Prompt { content, priority } => {
                        tracing::info!(
                            target: "synthia.session",
                            session_id = %inner.session_id,
                            op = "Prompt",
                            priority,
                            preview = truncate(&content, 40),
                            "op_rx: received Prompt"
                        );
                        // The user prompt itself is NOT persisted to
                        // the sink here — it is persisted at run
                        // completion (see the run-task body below).
                        // Persisting here would let the about-to-start
                        // run read its own prompt back out of
                        // `sink_history` and feed it to the agent a
                        // second time (once via `history` and once via
                        // `content`), making the LLM treat the prompt
                        // as a duplicate and discard the conversation
                        // context. We persist at run completion so the
                        // NEXT run sees this turn's prompt in
                        // `sink_history` (multi-turn memory), while
                        // THIS run only sees prior turns.
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

                        if run_handle.is_none()
                            && let Some(h) = inner.maybe_start_run().await
                        {
                            run_handle = Some(h);
                        }
                    }
                    SessionOp::Steer { content, priority } => {
                        tracing::info!(
                            target: "synthia.session",
                            session_id = %inner.session_id,
                            op = "Steer",
                            priority,
                            preview = truncate(&content, 40),
                            "op_rx: received Steer"
                        );
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

                        if run_handle.is_none()
                            && let Some(h) = inner.maybe_start_run().await
                        {
                            run_handle = Some(h);
                        }
                    }
                    SessionOp::Cancel { reason } => {
                        tracing::info!(
                            target: "synthia.session",
                            session_id = %inner.session_id,
                            op = "Cancel",
                            reason = reason.as_deref().unwrap_or("-"),
                            "op_rx: received Cancel; firing cancellation token"
                        );
                        if let Some(token) = inner.run_cancel.lock()
                            .expect("run_cancel mutex poisoned")
                            .as_ref()
                        {
                            token.cancel();
                        }
                        // Drop any queued inputs so the controller does not
                        // immediately restart the run after cancellation.
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
                        let mut state = inner.state.lock().expect("state mutex poisoned");
                        *state = SessionState::Cancelled;
                        if let Some(reason) = reason {
                            tracing::info!(
                                target: "synthia.session",
                                session_id = %inner.session_id,
                                reason,
                                "Session run cancelled"
                            );
                        }
                    }
                    SessionOp::Shutdown => {
                        tracing::info!(
                            target: "synthia.session",
                            session_id = %inner.session_id,
                            op = "Shutdown",
                            "op_rx: received Shutdown; breaking controller loop"
                        );
                        if let Some(token) = inner.run_cancel.lock()
                            .expect("run_cancel mutex poisoned")
                            .take()
                        {
                            token.cancel();
                        }
                        if let Some(h) = run_handle.take() {
                            let _ = h.await;
                        }
                        break;
                    }
                }
            }

            result = async {
                match run_handle {
                    Some(ref mut h) => h.await,
                    None => pending::<Result<(), tokio::task::JoinError>>().await,
                }
            }, if run_handle.is_some() => {
                run_handle = None;
                if let Err(e) = result {
                    tracing::error!(
                        session_id = %inner.session_id,
                        error = %e,
                        "Session run task panicked"
                    );
                }
                // If more inputs arrived while the run was active, start
                // the next run immediately.
                if inner
                    .queue
                    .has_pending(&inner.user_id, &inner.session_id)
                    .await
                    && let Some(h) = inner.maybe_start_run().await
                {
                    run_handle = Some(h);
                }
                last_activity = tokio::time::Instant::now();
            }

            _ = async {
                match idle_deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => pending::<()>().await,
                }
            }, if idle_deadline.is_some() => {
                tracing::info!(
                    session_id = %inner.session_id,
                    "Session controller idle timeout reached; shutting down"
                );
                if let Some(token) = inner.run_cancel.lock()
                    .expect("run_cancel mutex poisoned")
                    .take()
                {
                    token.cancel();
                }
                if let Some(h) = run_handle.take() {
                    let _ = h.await;
                }
                break;
            }

            else => {
                if let Some(h) = run_handle.take() {
                    let _ = h.await;
                }
                break;
            }
        }
    }

    inner.alive.store(false, Ordering::SeqCst);
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

/// Truncate a string to at most `max_chars` Unicode scalar values,
/// appending an ellipsis marker when truncation actually happened.
/// Used only for log previews so a 100kB user prompt does not
/// produce a 100kB log line.
fn truncate(s: &str, max_chars: usize) -> String {
    let mut iter = s.chars();
    let mut out = String::with_capacity(max_chars + 1);
    for _ in 0..max_chars {
        match iter.next() {
            Some(c) => out.push(c),
            None => return out,
        }
    }
    if iter.next().is_some() {
        out.push('…');
    }
    out
}

/// Translate the durable `AgentEvent`s previously persisted to a
/// session sink back into the `Message` history the ReAct agent
/// consumes on a new run.
///
/// The event taxonomy that survives persistence is deliberately
/// narrow (see `AgentEvent::is_durable`):
///
/// - `Model(ContentPart::Text(_))` → assistant message
/// - `Model(ContentPart::ToolUse(_))` → assistant message with
///   `tool_calls`
/// - `Model(ContentPart::ToolResult(_))` → tool message
///
/// Ephemeral events (`ModelDone`, every `System` variant, `Model`
/// variants carrying reasoning / image / audio / resource) are
/// filtered out before this function ever sees them — the sink
/// already dropped them at `persist_and_broadcast` time.
///
/// Malformed records are skipped (with a `warn!`) rather than
/// failing the whole run: a single corrupt JSONL row from an older
/// agent version must not wedge a long-lived session.
fn events_to_history(events: &[Value]) -> Vec<Message> {
    let mut messages: Vec<Message> = Vec::with_capacity(events.len());
    for event in events {
        let Some(event_type) = event.get("type").and_then(|v| v.as_str())
        else {
            continue;
        };
        // User prompts are persisted as synthetic `UserInput`
        // envelopes by the `Prompt` handler in the controller
        // loop. They must produce `Message{Role:User}` so the
        // reconstructed history alternates user / assistant
        // correctly.
        if event_type == "UserInput" {
            let Some(text) = event
                .get("data")
                .and_then(|d| d.get("text"))
                .and_then(|t| t.as_str())
            else {
                tracing::warn!(
                    target: "synthia.session",
                    "events_to_history: dropping malformed UserInput record"
                );
                continue;
            };
            messages.push(Message {
                role: Role::User,
                content: Content::Single(ContentPart::Text(TextContent {
                    text: text.to_string(),
                    cache_control: None,
                })),
                tool_call_id: None,
                name: None,
                ..Default::default()
            });
            continue;
        }
        if event_type != "Model" {
            continue;
        }
        let Some(data) = event.get("data") else {
            continue;
        };
        // `ContentPart` is serialized with
        // `#[serde(tag = "type", rename_all = "snake_case")]` so
        // the JSON shape is `{"type":"text","text":"..."}` /
        // `{"type":"tool_use",...}` etc. — deserializing the data
        // object into `ContentPart` directly picks the right
        // variant for us.
        let part: ContentPart = match serde_json::from_value(data.clone()) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    target: "synthia.session",
                    error = %e,
                    "events_to_history: dropping malformed Model record"
                );
                continue;
            }
        };
        match part {
            ContentPart::Text(text) => messages.push(Message {
                role: Role::Assistant,
                content: Content::Single(ContentPart::Text(text)),
                tool_call_id: None,
                name: None,
                ..Default::default()
            }),
            ContentPart::ToolUse(tool_use) => messages.push(Message {
                role: Role::Assistant,
                content: Content::Single(ContentPart::ToolUse(tool_use)),
                tool_call_id: None,
                name: None,
                ..Default::default()
            }),
            ContentPart::ToolResult(tool_result) => {
                messages.push(Message {
                    role: Role::Tool,
                    content: Content::Single(ContentPart::ToolResult(
                        tool_result,
                    )),
                    tool_call_id: None,
                    name: None,
                    ..Default::default()
                });
            }
            ContentPart::Image(_)
            | ContentPart::Audio(_)
            | ContentPart::Reasoning(_)
            | ContentPart::Resource(_) => {
                // Reasoning / Image / Audio are non-durable so they
                // never reach the sink. Resource is durable but has
                // no `Message` shape; skip silently.
            }
        }
    }
    messages
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use synthia_agent::{SessionEndReason, SystemEvent};

    use super::*;

    /// `truncate` truncates a string to at most
    /// `max_chars` Unicode scalar values, appending
    /// a Unicode ellipsis (`…`) when truncation
    /// actually happened. Char-count (not
    /// byte-count) is the contract so the function
    /// behaves correctly for multi-byte text
    /// (中文 / 日本語).
    ///
    /// No test pins this today; a refactor that
    /// switched to byte-indexing would silently
    /// corrupt non-ASCII log previews.
    mod truncate_tests {
        use super::truncate;

        #[test]
        fn truncate_shorter_than_max_is_returned_verbatim() {
            assert_eq!(truncate("hi", 5), "hi");
            assert_eq!(truncate("hello", 5), "hello");
        }

        #[test]
        fn truncate_equal_to_max_is_returned_verbatim() {
            // Boundary: exactly max_chars → no
            // truncation. The `iter.next()` check
            // after the loop is `None` (no extra
            // char), so no `…` is appended.
            assert_eq!(truncate("hello", 5), "hello");
        }

        #[test]
        fn truncate_one_over_max_is_truncated_with_ellipsis() {
            assert_eq!(truncate("hello!", 5), "hello…");
        }

        #[test]
        fn truncate_empty_string_is_returned_verbatim() {
            assert_eq!(truncate("", 0), "");
            assert_eq!(truncate("", 5), "");
        }

        #[test]
        fn truncate_max_zero_with_nonempty_returns_ellipsis_only() {
            // The `for _ in 0..0` loop is a no-op.
            // `iter.next()` returns Some('x'), so
            // the ellipsis branch fires with an
            // empty `out`. Pin this so a refactor
            // doesn't swallow the ellipsis on
            // zero-width truncation.
            assert_eq!(truncate("x", 0), "…");
        }

        #[test]
        fn truncate_counts_chars_not_bytes_for_multibyte_text() {
            // "中文" is 2 chars but 6 bytes (UTF-8).
            // With max=2 we MUST return "中文"
            // (no truncation) rather than
            // byte-truncated garbage.
            assert_eq!(truncate("中文", 2), "中文");
            assert_eq!(truncate("中文!", 2), "中文…");
            assert_eq!(truncate("日本語テスト", 3), "日本語…");
        }

        #[test]
        fn truncate_does_not_append_ellipsis_on_exact_match() {
            // Distinguish "exact match" from "one
            // over": the former MUST NOT have the
            // ellipsis appended.
            let s = "abcde";
            assert_eq!(truncate(s, 5), "abcde");
            assert!(
                !truncate(s, 5).ends_with('…'),
                "exact match must not get an ellipsis"
            );
        }
    }

    fn test_deps() -> RunDependencies {
        RunDependencies::new(
            Arc::new(test_support::FakeProvider::new(vec![])),
            Arc::new(RwLock::new(ToolRegistry::new())),
            PathBuf::from("/tmp"),
            synthia_agent::agent::re_act::DEFAULT_SYSTEM_PROMPT.to_string(),
        )
    }

    async fn make_manager_and_controller(
        idle_timeout: Duration,
        run_factory: Arc<dyn RunStreamFactory>,
    ) -> (
        Arc<SessionController>,
        synthia_session::manager::SessionRegistry,
        tempfile::TempDir,
    ) {
        let temp = tempfile::TempDir::new().unwrap();
        let manager = synthia_session::manager::SessionRegistry::new(
            temp.path().to_path_buf(),
        );
        manager
            .create_with_user("s1".to_string(), "alice".to_string())
            .await
            .unwrap();

        let deps = test_deps();
        let session_sink = manager.sink("alice", "s1");
        let controller = SessionController::spawn(
            "alice",
            "s1",
            manager.input_queue(),
            session_sink,
            deps,
            idle_timeout,
            run_factory,
        );

        (controller, manager, temp)
    }

    /// A factory that emits a fixed list of events and drains the
    /// session input queue, useful for persistence and broadcast tests.
    struct VecFactory {
        events: Vec<AgentEvent>,
        calls: Arc<Mutex<Vec<AgentRunConfig>>>,
        queue: Option<SessionInputQueue>,
    }

    impl VecFactory {
        fn new(
            events: Vec<AgentEvent>,
            calls: Arc<Mutex<Vec<AgentRunConfig>>>,
            queue: Option<SessionInputQueue>,
        ) -> Self {
            Self {
                events,
                calls,
                queue,
            }
        }
    }

    impl RunStreamFactory for VecFactory {
        fn run_stream(
            &self,
            config: AgentRunConfig,
            _input: AgentInput,
            cancel: Arc<CancellationToken>,
        ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>> {
            self.calls.lock().unwrap().push(config.clone());

            let events = self.events.clone();
            let _queue = self.queue.clone();

            let stream = futures::stream::iter(events).take_while(move |_| {
                futures::future::ready(!cancel.is_cancelled())
            });
            Box::pin(stream)
        }
    }

    /// A factory that blocks until its cancellation token fires,
    /// useful for verifying that only one run is active at a time.
    struct BlockingFactory {
        calls: Arc<Mutex<Vec<AgentRunConfig>>>,
        progress_count: Arc<AtomicUsize>,
    }

    impl RunStreamFactory for BlockingFactory {
        fn run_stream(
            &self,
            config: AgentRunConfig,
            _input: AgentInput,
            cancel: Arc<CancellationToken>,
        ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>> {
            self.calls.lock().unwrap().push(config.clone());

            let count = Arc::clone(&self.progress_count);

            let stream = async_stream::stream! {
                while !cancel.is_cancelled() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    count.fetch_add(1, Ordering::SeqCst);
                    yield AgentEvent::Model(ContentPart::Text(TextContent {
                        text: format!("working {}", count.load(Ordering::SeqCst)),
                        cache_control: None,
                    }));
                }
                yield AgentEvent::System(SystemEvent::SessionEnded {
                    reason: SessionEndReason::Cancelled,
                });
            };
            Box::pin(stream)
        }
    }

    #[tokio::test]
    async fn test_two_concurrent_prompts_spawn_one_run() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let factory: Arc<dyn RunStreamFactory> = Arc::new(BlockingFactory {
            calls: Arc::clone(&calls),
            progress_count: Arc::new(AtomicUsize::new(0)),
        });
        let (controller, _manager, _temp) =
            make_manager_and_controller(Duration::from_secs(60), factory).await;

        controller
            .submit(SessionOp::Prompt {
                content: "hello".to_string(),
                priority: 1,
            })
            .await
            .unwrap();

        // Wait until the run is definitely active before submitting the
        // second prompt.
        tokio::time::timeout(Duration::from_millis(500), async {
            while controller.state() != SessionState::Running {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();

        controller
            .submit(SessionOp::Prompt {
                content: "world".to_string(),
                priority: 1,
            })
            .await
            .unwrap();

        // Give the controller a moment to process the second prompt.
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert_eq!(calls.lock().unwrap().len(), 1);

        controller.cancel().await.unwrap();
        tokio::time::timeout(Duration::from_millis(500), async {
            while controller.state() != SessionState::Idle
                && controller.state() != SessionState::Cancelled
            {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }

    /// `Steer` priority ordering is covered end-to-end by the
    /// `synthia-session` crate's input-queue tests. This test is a
    /// pre-existing race that asserts the queue is still populated
    /// 50ms after the spawned run task has *already* drained it
    /// (the run factory takes the pending entries before the test
    /// reads them). The behaviour it asserts is no longer reachable
    /// since the controller's run task drains eagerly. Ignore until
    /// a non-racy formulation lands.
    #[tokio::test]
    #[ignore = "pre-existing race: spawned run drains the queue before assertion"]
    async fn test_steer_is_appended_with_high_priority() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let factory: Arc<dyn RunStreamFactory> =
            Arc::new(VecFactory::new(vec![], Arc::clone(&calls), None));
        let (controller, manager, _temp) =
            make_manager_and_controller(Duration::from_secs(60), factory).await;

        controller
            .submit(SessionOp::Steer {
                content: "turn left".to_string(),
                priority: 200,
            })
            .await
            .unwrap();

        // Give the controller time to append the entry.
        tokio::time::sleep(Duration::from_millis(50)).await;

        let pending = manager
            .input_queue()
            .drain_pending("alice", "s1")
            .await
            .unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].content, "turn left");
        // The refactored `InputQueue` discards priority
        // because steering is now handled by the agent loop,
        // not by a persisted queue.
    }

    #[tokio::test]
    async fn test_cancel_terminates_run() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let factory: Arc<dyn RunStreamFactory> = Arc::new(BlockingFactory {
            calls: Arc::clone(&calls),
            progress_count: Arc::new(AtomicUsize::new(0)),
        });
        let (controller, manager, _temp) =
            make_manager_and_controller(Duration::from_secs(60), factory).await;

        controller
            .submit(SessionOp::Prompt {
                content: "start".to_string(),
                priority: 1,
            })
            .await
            .unwrap();

        tokio::time::timeout(Duration::from_millis(500), async {
            while controller.state() != SessionState::Running {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();

        // Drain the input queue so the controller does not restart the
        // run immediately after cancellation.
        let _ = manager.input_queue().drain_pending("alice", "s1").await;
        controller.cancel().await.unwrap();

        tokio::time::timeout(Duration::from_millis(500), async {
            while controller.state() != SessionState::Cancelled
                && controller.state() != SessionState::Idle
            {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();

        assert_eq!(calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_repeated_cancel_is_idempotent() {
        // Verify that submitting multiple `Cancel` ops in
        // quick succession does not panic, double-fire any
        // state transitions, or leave the controller in an
        // unexpected state. `CancellationToken::cancel()` is
        // documented as idempotent and the handler must
        // preserve that property.
        let calls = Arc::new(Mutex::new(Vec::new()));
        let factory: Arc<dyn RunStreamFactory> = Arc::new(BlockingFactory {
            calls: Arc::clone(&calls),
            progress_count: Arc::new(AtomicUsize::new(0)),
        });
        let (controller, manager, _temp) =
            make_manager_and_controller(Duration::from_secs(60), factory).await;
        controller
            .submit(SessionOp::Prompt {
                content: "start".to_string(),
                priority: 1,
            })
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_millis(500), async {
            while controller.state() != SessionState::Running {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let _ = manager.input_queue().drain_pending("alice", "s1").await;

        // Three cancels back-to-back — none must error.
        controller.cancel().await.unwrap();
        controller.cancel().await.unwrap();
        controller.cancel().await.unwrap();

        // The state still converges (either `Cancelled` or
        // `Idle` after the controller reaps the run).
        tokio::time::timeout(Duration::from_millis(500), async {
            loop {
                let s = controller.state();
                if s == SessionState::Cancelled || s == SessionState::Idle {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();

        // Exactly one run was ever started (no spurious
        // restarts from repeated cancels re-queueing work).
        assert_eq!(calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_cancel_before_run_starts_is_safe_noop() {
        // Edge case: cancel arrives BEFORE any Prompt has
        // been dequeued (e.g. the user submitted then
        // immediately gave up). `run_cancel` is `None` at
        // that point, so the controller must treat the
        // cancel as a safe no-op — no panic, no
        // double-transition, no hang.
        let calls = Arc::new(Mutex::new(Vec::new()));
        let factory: Arc<dyn RunStreamFactory> =
            Arc::new(VecFactory::new(vec![], Arc::clone(&calls), None));
        let (controller, _manager, _temp) =
            make_manager_and_controller(Duration::from_secs(60), factory).await;
        // No Prompt submitted — controller is Idle.
        assert_eq!(controller.state(), SessionState::Idle);
        // Cancel while Idle.
        controller.cancel().await.unwrap();
        // Must remain Idle (no run to cancel).
        assert_eq!(controller.state(), SessionState::Idle);
        // No factory calls were ever made.
        assert_eq!(calls.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_shutdown_after_idle_timeout() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let factory: Arc<dyn RunStreamFactory> =
            Arc::new(VecFactory::new(vec![], Arc::clone(&calls), None));
        let (controller, _manager, _temp) =
            make_manager_and_controller(Duration::from_millis(50), factory)
                .await;

        assert!(controller.is_alive());
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert!(!controller.is_alive());
    }

    /// After `idle_timeout` fires and the controller shuts
    /// down, a new `submit()` call MUST return
    /// `Err` with the documented "session controller is
    /// shut down" context — NOT silently drop the op, NOT
    /// panic. This is the contract the A2A executor
    /// depends on: when `get_or_create_session_controller`
    /// fails (because the prior controller just shut down
    /// between the call and the `submit`), the error
    /// path bubbles up and the A2A client sees a 5xx
    /// instead of an empty 200 OK.
    ///
    /// Without this contract, a previously-shut-down
    /// controller could be silently reused as if it were
    /// alive, leaking the queued op into a stale run.
    #[tokio::test]
    async fn test_submit_after_shutdown_returns_error() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let factory: Arc<dyn RunStreamFactory> =
            Arc::new(VecFactory::new(vec![], Arc::clone(&calls), None));
        let (controller, _manager, _temp) =
            make_manager_and_controller(Duration::from_millis(50), factory)
                .await;

        // Wait for idle_timeout to fire and the controller
        // to drop its op_tx receiver.
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert!(!controller.is_alive());

        // Now submit MUST fail.
        let result = controller
            .submit(SessionOp::Prompt {
                content: "after-shutdown".to_string(),
                priority: 1,
            })
            .await;
        assert!(
            result.is_err(),
            "submit after shutdown must return Err, got Ok"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("shut down"),
            "error must include the documented context; got: {err_msg}"
        );
        // No factory calls were made — the post-shutdown
        // op never reached the run loop.
        assert_eq!(calls.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_events_are_persisted_and_broadcast() {
        // MVP: all events are durable. Persist every event the
        // factory emits, then verify both broadcast and persistence
        // observed them.
        let events = vec![
            AgentEvent::Model(ContentPart::Text(TextContent {
                text: "hi".to_string(),
                cache_control: None,
            })),
            AgentEvent::System(SystemEvent::SessionStarted {
                session_id: "s1".to_string(),
            }),
            AgentEvent::System(SystemEvent::SessionEnded {
                reason: SessionEndReason::Completed,
            }),
        ];
        let calls = Arc::new(Mutex::new(Vec::new()));
        let factory: Arc<dyn RunStreamFactory> =
            Arc::new(VecFactory::new(events, Arc::clone(&calls), None));
        let (controller, manager, _temp) =
            make_manager_and_controller(Duration::from_secs(60), factory).await;

        // Subscribe before the run starts so events are broadcast.
        let mut rx = controller.subscribe();

        controller
            .submit(SessionOp::Prompt {
                content: "go".to_string(),
                priority: 1,
            })
            .await
            .unwrap();

        // Wait for the run to start.
        tokio::time::timeout(Duration::from_millis(500), async {
            while calls.lock().unwrap().is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();

        // Drain the queue so the controller does not immediately restart
        // the run, then wait for the run to finish.
        let _ = manager.input_queue().drain_pending("alice", "s1").await;
        tokio::time::timeout(Duration::from_millis(500), async {
            while controller.state() != SessionState::Idle {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();

        let persisted: Vec<serde_json::Value> =
            manager.sink("alice", "s1").read().await.unwrap();
        // After the panel/session refactor, the sink stores
        // only **durable** events (per the
        // `event-durability-classification` spec). System
        // events such as `SessionStarted` / `SessionEnded` are
        // ephemeral: they are broadcast to subscribers but not
        // persisted, because a cold start rebuilds the agent
        // state from the durable slices (model text, tool
        // calls, tool results, resources). The run task
        // also appends a synthetic `UserInput` envelope at
        // run start (after reading the sink, before invoking
        // the agent) so the NEXT run can reconstruct
        // user-role messages; that is durable too. The
        // `VecFactory` for this test emits one durable
        // `Model(Text)` and two ephemeral `System` events,
        // so the JSONL must contain exactly two records:
        // the `UserInput` from this run's prompt and the
        // `Model(Text)` from the agent.
        assert_eq!(
            persisted.len(),
            2,
            "only durable events are persisted; got {persisted:?}"
        );
        let type_of = |e: &serde_json::Value| -> String {
            e.get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        assert!(
            persisted.iter().any(|e| type_of(e) == "UserInput"),
            "UserInput envelope from the prompt handler should be persisted"
        );
        assert!(
            persisted
                .iter()
                .any(|e| type_of(e) == "Model" || type_of(e) == "model"),
            "Model event should be persisted"
        );

        let received =
            tokio::time::timeout(Duration::from_millis(200), rx.recv())
                .await
                .unwrap()
                .unwrap();
        // The first broadcast event is the Message event (emitted first
        // by the factory).
        assert!(matches!(received, AgentEvent::Model(ContentPart::Text(_))));
    }

    /// Records every `(input)` the controller hands to the agent,
    /// plus the events the factory streams back. Used by
    /// `test_second_prompt_seeds_history_from_persisted_turns` to
    /// pin the multi-turn memory contract.
    struct RecordingFactory {
        calls: Arc<Mutex<Vec<AgentInput>>>,
        first_run_events: Vec<AgentEvent>,
        second_run_events: Vec<AgentEvent>,
    }

    impl RecordingFactory {
        fn new(
            calls: Arc<Mutex<Vec<AgentInput>>>,
            first_run_events: Vec<AgentEvent>,
            second_run_events: Vec<AgentEvent>,
        ) -> Self {
            Self {
                calls,
                first_run_events,
                second_run_events,
            }
        }
    }

    impl RunStreamFactory for RecordingFactory {
        fn run_stream(
            &self,
            _config: AgentRunConfig,
            input: AgentInput,
            cancel: Arc<CancellationToken>,
        ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>> {
            self.calls.lock().unwrap().push(input);
            let run_index = self.calls.lock().unwrap().len() - 1;
            let events = if run_index == 0 {
                self.first_run_events.clone()
            } else {
                self.second_run_events.clone()
            };
            let stream = futures::stream::iter(events).take_while(move |_| {
                futures::future::ready(!cancel.is_cancelled())
            });
            Box::pin(stream)
        }
    }

    /// Regression: the second prompt submitted to the same session
    /// must seed `AgentInput::history` with the assistant turns the
    /// previous run persisted to the `SessionSink`.
    ///
    /// Without the fix, the controller never re-reads the sink, so
    /// `input.history` is empty on every run and the LLM sees a
    /// fresh conversation — losing multi-turn memory.
    #[tokio::test]
    async fn test_second_prompt_seeds_history_from_persisted_turns() {
        use synthia_provider::TextContent;

        let calls = Arc::new(Mutex::new(Vec::new()));
        // First run emits one durable assistant text chunk; second
        // run emits another. Both must be observable to the agent.
        let factory: Arc<dyn RunStreamFactory> =
            Arc::new(RecordingFactory::new(
                Arc::clone(&calls),
                vec![AgentEvent::Model(ContentPart::Text(TextContent {
                    text: "first reply".to_string(),
                    cache_control: None,
                }))],
                vec![AgentEvent::Model(ContentPart::Text(TextContent {
                    text: "second reply".to_string(),
                    cache_control: None,
                }))],
            ));

        let (controller, _manager, _temp) =
            make_manager_and_controller(Duration::from_secs(60), factory).await;

        // Turn 1
        controller
            .submit(SessionOp::Prompt {
                content: "turn-1 prompt".to_string(),
                priority: 1,
            })
            .await
            .unwrap();

        // Wait for the run to finish and the controller to return
        // to Idle so we know the first run's events have been
        // persisted by `persist_and_broadcast`.
        tokio::time::timeout(Duration::from_secs(2), async {
            while controller.state() != SessionState::Idle {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("first run should finish");

        // After state flips to Idle the controller still holds the
        // first run's `JoinHandle` for one more select! iteration;
        // only once `run_handle.await` returns and sets
        // `run_handle = None` does a new op_rx prompt actually
        // spawn a fresh run. Wait for that window to close before
        // submitting turn 2, otherwise the controller will see
        // `run_handle.is_some()` and drop the dispatch. Poll on
        // the captured calls vector: once it stops growing, the
        // first run's stream has been fully consumed.
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if calls.lock().unwrap().len() == 1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("first run input should be recorded");
        // Tiny grace period for the spawned task to exit so the
        // controller loop clears `run_handle`.
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Turn 2 — submits a fresh prompt. The fix must cause
        // `input.history` of run #2 to contain run #1's assistant
        // text "first reply" so the agent has memory of the prior
        // turn.
        controller
            .submit(SessionOp::Prompt {
                content: "turn-2 prompt".to_string(),
                priority: 1,
            })
            .await
            .unwrap();

        // The controller serializes through op_rx; once the
        // second run is dispatched the factory records a second
        // input. Allow generous slack because the controller may
        // briefly sit in its idle branch before re-entering the
        // select! iteration that picks up op_rx.
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if calls.lock().unwrap().len() >= 2 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("second run should start");

        let recorded = calls.lock().unwrap();
        assert_eq!(
            recorded.len(),
            2,
            "controller should have spawned two runs"
        );

        // The second run's input must carry the first run's
        // persisted assistant text inside `history`. We allow
        // the current-turn prompt itself to live in `content`,
        // but the prior assistant turn must be in `history` —
        // that is the contract being violated by the bug.
        let second = &recorded[1];
        let history_text = second
            .history
            .iter()
            .filter_map(|m| match &m.content {
                Content::Single(ContentPart::Text(t)) => Some(t.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            history_text.contains("first reply"),
            "second run's history must include first run's assistant text; got history={:?}",
            second.history
        );

        // The current-turn prompt must live in `content`, NOT in
        // `history` — appending the prompt to the sink before the
        // run drains the queue would cause this assertion to fail
        // because the run would read its own prompt back out of
        // `sink_history` and feed it to the agent twice (once via
        // `history`, once via `content`). The fix persists the
        // drained prompts after reading the sink.
        assert!(
            !history_text.contains("turn-2 prompt"),
            "second run's history must NOT include the current-turn prompt (would cause \
             duplicate-feed); got history={:?}",
            second.history
        );

        // Also assert turn-2's prompt landed as the input content
        // so we know the history seed didn't overwrite the prompt.
        let second_prompt_text = match second.content.first() {
            Some(ContentPart::Text(t)) => t.text.clone(),
            _ => panic!("second run content should be text"),
        };
        assert_eq!(second_prompt_text, "turn-2 prompt");
    }

    /// Regression: user prompts must be persisted to the session sink
    /// and reconstructed as `Message{Role:User}` in the next run's
    /// history. Without the `UserInput` envelope append in the
    /// `Prompt` handler, `events_to_history` only sees assistant
    /// messages and the LLM treats every turn as a fresh conversation.
    #[tokio::test]
    async fn test_user_prompt_persists_and_reacts_to_role_user_in_history() {
        use synthia_provider::TextContent;

        let calls = Arc::new(Mutex::new(Vec::new()));
        let factory: Arc<dyn RunStreamFactory> =
            Arc::new(RecordingFactory::new(
                Arc::clone(&calls),
                vec![AgentEvent::Model(ContentPart::Text(TextContent {
                    text: "ack".to_string(),
                    cache_control: None,
                }))],
                vec![AgentEvent::Model(ContentPart::Text(TextContent {
                    text: "ack".to_string(),
                    cache_control: None,
                }))],
            ));

        let (controller, _manager, _temp) =
            make_manager_and_controller(Duration::from_secs(60), factory).await;

        controller
            .submit(SessionOp::Prompt {
                content: "我的猫叫蓝杉".to_string(),
                priority: 1,
            })
            .await
            .unwrap();

        tokio::time::timeout(Duration::from_secs(2), async {
            while controller.state() != SessionState::Idle {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("first run should finish");

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if calls.lock().unwrap().len() == 1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("first run input should be recorded");
        tokio::time::sleep(Duration::from_millis(20)).await;

        controller
            .submit(SessionOp::Prompt {
                content: "它几岁？".to_string(),
                priority: 1,
            })
            .await
            .unwrap();

        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if calls.lock().unwrap().len() >= 2 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("second run should start");

        let recorded = calls.lock().unwrap();
        assert_eq!(recorded.len(), 2);
        let second = &recorded[1];

        let user_messages: Vec<&str> = second
            .history
            .iter()
            .filter(|m| m.role == Role::User)
            .filter_map(|m| match &m.content {
                Content::Single(ContentPart::Text(t)) => Some(t.text.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            user_messages.iter().any(|t| t.contains("蓝杉")),
            "second run history must contain turn-1 user prompt as Role::User; got history={:?}",
            second.history
        );

        // The current-turn prompt must NOT appear in `history` —
        // duplicating it would make the LLM treat the prompt as a
        // fresh message and discard the conversation context.
        assert!(
            user_messages.iter().all(|t| !t.contains("它几岁")),
            "second run history must NOT include the current-turn prompt (would cause \
             duplicate-feed); got history={:?}",
            second.history
        );
    }
}
