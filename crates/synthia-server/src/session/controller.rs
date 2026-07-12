//! Per-session controller that serializes prompt/steer/cancel operations
//! and ensures at most one `Agent::run_stream` per session.

use std::{
    future::pending,
    path::PathBuf,
    sync::{
        Arc,
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context as _, Result};
use futures::StreamExt;
use synthia_agent::{
    Agent,
    AgentConfig,
    AgentEvent,
    AgentInput,
    AgentOutput,
    AgentRunConfig,
    SubagentSessionFactory,
    control::AgentControl,
    tools::{AgentTool, agent_tools::team::SubagentManager},
};
use synthia_context::{ProtectionZone, assembler::ContextAssembler};
use synthia_hook::HookRegistry;
use synthia_permission::ApprovalService;
use synthia_provider::{router::ModelRouter, traits::ModelProvider};
use synthia_sandbox::SandboxManager;
use synthia_session::{
    Store as SessionStore,
    store::{EventSource, SessionInputQueue},
};
use synthia_tool::registry::ToolRegistry;
use synthia_tool_orchestrator::ToolOrchestrator;
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
    fn run_stream(&self, config: AgentRunConfig) -> AgentOutput;
}

/// Production implementation that delegates to [`Agent::run_stream`].
#[derive(Debug, Clone, Default)]
pub struct AgentRunStreamFactory;

impl RunStreamFactory for AgentRunStreamFactory {
    fn run_stream(&self, config: AgentRunConfig) -> AgentOutput {
        Agent::run_stream(config)
    }
}

/// Dependencies required to build an [`AgentRunConfig`] for the session.
#[derive(Clone)]
pub struct RunDependencies {
    pub provider: Arc<dyn ModelProvider>,
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
    pub session_store: SessionStore,
    pub workspace_root: PathBuf,
    pub default_model: String,
    pub subagent_factory: Arc<dyn SubagentSessionFactory>,
    pub approval_service: Arc<dyn ApprovalService>,
    pub sandbox_manager: Arc<dyn SandboxManager>,
    pub tool_orchestrator: Arc<dyn ToolOrchestrator>,
    pub agent_control: Arc<AgentControl>,
    /// Spawn depth to apply to the session's [`SubagentManager`].
    ///
    /// Root sessions have depth 0; direct children have depth 1, etc.
    /// `build_run_config` calls `manager.set_depth(subagent_depth)` so
    /// that `AgentTool::call` can enforce `max_depth` for nested spawns.
    pub subagent_depth: usize,
}

impl RunDependencies {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: Arc<dyn ModelProvider>,
        tool_registry: Arc<RwLock<ToolRegistry>>,
        session_store: SessionStore,
        workspace_root: PathBuf,
        default_model: String,
        subagent_factory: Arc<dyn SubagentSessionFactory>,
        approval_service: Arc<dyn ApprovalService>,
        sandbox_manager: Arc<dyn SandboxManager>,
        tool_orchestrator: Arc<dyn ToolOrchestrator>,
        agent_control: Arc<AgentControl>,
        subagent_depth: usize,
    ) -> Self {
        Self {
            provider,
            tool_registry,
            session_store,
            workspace_root,
            default_model,
            subagent_factory,
            approval_service,
            sandbox_manager,
            tool_orchestrator,
            agent_control,
            subagent_depth,
        }
    }
}

/// Shared handle to a running session controller.
pub struct SessionController {
    session_id: String,
    user_id: String,
    state: Arc<Mutex<SessionState>>,
    op_tx: mpsc::Sender<SessionOp>,
    event_tx: mpsc::Sender<AgentEvent>,
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
        session_path: PathBuf,
        broadcaster: EventBroadcaster,
        deps: RunDependencies,
        idle_timeout: Duration,
        run_factory: Arc<dyn RunStreamFactory>,
        parent_event_sender: Option<mpsc::Sender<AgentEvent>>,
    ) -> Arc<Self> {
        let (op_tx, op_rx) = mpsc::channel(64);
        let (event_tx, event_rx) = mpsc::channel(64);
        let state = Arc::new(Mutex::new(SessionState::Idle));
        let alive = Arc::new(AtomicBool::new(true));

        let controller = Arc::new(Self {
            session_id: session_id.into(),
            user_id: user_id.into(),
            state: state.clone(),
            op_tx,
            event_tx: event_tx.clone(),
            broadcaster: broadcaster.clone(),
            alive: alive.clone(),
        });

        let inner = Arc::new(ControllerInner {
            session_id: controller.session_id.clone(),
            user_id: controller.user_id.clone(),
            state,
            queue,
            session_path,
            broadcaster,
            deps,
            idle_timeout,
            run_cancel: Mutex::new(None),
            run_factory,
            alive,
            parent_event_sender,
        });

        tokio::spawn(run_controller_loop(inner, op_rx, event_rx));

        controller
    }

    /// Returns a clone of the controller's forwarded-event channel sender.
    pub fn event_sender(&self) -> mpsc::Sender<AgentEvent> {
        self.event_tx.clone()
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
    session_path: PathBuf,
    broadcaster: EventBroadcaster,
    deps: RunDependencies,
    idle_timeout: Duration,
    run_cancel: Mutex<Option<CancellationToken>>,
    run_factory: Arc<dyn RunStreamFactory>,
    alive: Arc<AtomicBool>,
    parent_event_sender: Option<mpsc::Sender<AgentEvent>>,
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
                return None;
            }
        }

        if !self.queue.has_pending(&self.user_id, &self.session_id) {
            return None;
        }

        {
            let mut state = self.state.lock().expect("state mutex poisoned");
            *state = SessionState::Running;
        }

        let cancel_token = CancellationToken::new();
        *self.run_cancel.lock().expect("run_cancel mutex poisoned") =
            Some(cancel_token.clone());

        let config = self.build_run_config(cancel_token);
        let factory = Arc::clone(&self.run_factory);
        let inner = Arc::clone(self);

        Some(tokio::spawn(async move {
            let mut stream = factory.run_stream(config);
            while let Some(event) = stream.next().await {
                if let Err(e) = inner.persist_and_broadcast(&event).await {
                    tracing::error!(
                        session_id = %inner.session_id,
                        error = %e,
                        "Failed to persist or broadcast event"
                    );
                }
            }

            let mut state = inner.state.lock().expect("state mutex poisoned");
            if *state == SessionState::Running {
                *state = SessionState::Idle;
            }
        }))
    }

    fn build_run_config(
        &self,
        cancel_token: CancellationToken,
    ) -> AgentRunConfig {
        let config = AgentConfig {
            model: self.deps.default_model.clone(),
            max_iterations: 20,
            max_tokens: 4096,
            temperature: Some(0.7),
            workspace_root: self.deps.workspace_root.clone(),
            token_budget: None,
            checkpoint_dir: None,
            context_token_budget: Some(
                synthia_session::types::TokenBudget::default(),
            ),
            ..Default::default()
        };

        let tool_registry = self
            .deps
            .tool_registry
            .try_read()
            .map(|r| (*r).clone())
            .unwrap_or_else(|_| ToolRegistry::new());

        // Register the subagent `task` tool. The shared AppState registry
        // is built before the runtime control plane / child-session factory
        // are available, so the tool is added per-session here. Propagate
        // the configured spawn depth so `AgentTool::call` can enforce
        // `max_depth` for nested spawns in production.
        let manager = Arc::new(SubagentManager::new());
        manager.set_depth(self.deps.subagent_depth);
        let agent_tool = Arc::new(AgentTool::new(manager, true));
        tool_registry.register(synthia_tool::ToolEntry::new(agent_tool));

        let protection_zone = ProtectionZone::default();
        let assembler = ContextAssembler::new(config.max_tokens)
            .with_protection_zone(protection_zone);

        AgentRunConfig {
            provider: Arc::clone(&self.deps.provider),
            tool_registry,
            hook_registry: Arc::new(HookRegistry::new()),
            model_router: Arc::new(ModelRouter::new()),
            user_id: self.user_id.clone(),
            session_id: self.session_id.clone(),
            input: AgentInput::text(""),
            config,
            context_assembler: Some(Arc::new(assembler)),
            session_store: self.deps.session_store.clone(),
            steering_channel: None,
            session_input_queue: Some(self.queue.clone()),
            cancel_token,
            memory_event_sender: None,
            agent_control: Some((*self.deps.agent_control).clone()),
            fork_policy: Default::default(),
            compaction_provider: None,
            subagent_session_factory: Some(self.deps.subagent_factory.clone()),
            approval_service: Some(Arc::clone(&self.deps.approval_service)),
            sandbox_manager: Some(Arc::clone(&self.deps.sandbox_manager)),
            tool_orchestrator: Some(Arc::clone(&self.deps.tool_orchestrator)),
            guardian_coordinator: None,
            extension_manager: None,
        }
    }

    async fn persist_and_broadcast(&self, event: &AgentEvent) -> Result<()> {
        let payload = serde_json::to_value(event)
            .context("failed to serialize agent event")?;
        let event_type = payload
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("AgentEvent");

        self.deps
            .session_store
            .event_store()
            .append(
                &self.session_path,
                &self.session_id,
                event_type,
                EventSource::Agent,
                !event.is_durable(),
                &payload,
            )
            .context("failed to append event to event store")?;

        if let Err(e) = self.broadcaster.send(event.clone()) {
            tracing::debug!(
                session_id = %self.session_id,
                error = %e,
                "No subscribers to broadcast event to"
            );
        }

        // Forward raw child events to the parent controller, if any.
        // This is best-effort: a closed parent channel must not break
        // the child session.
        if let Some(ref parent_tx) = self.parent_event_sender {
            let wrapped = AgentEvent::SubagentEvent {
                child_session_id: self.session_id.clone(),
                event: Box::new(event.clone()),
            };
            if let Err(e) = parent_tx.send(wrapped).await {
                tracing::warn!(
                    session_id = %self.session_id,
                    error = %e,
                    "Parent event channel closed; dropping forwarded subagent event"
                );
            }
        }

        Ok(())
    }
}

async fn run_controller_loop(
    inner: Arc<ControllerInner>,
    mut op_rx: mpsc::Receiver<SessionOp>,
    mut event_rx: mpsc::Receiver<AgentEvent>,
) {
    let mut last_activity = tokio::time::Instant::now();
    let mut run_handle: Option<tokio::task::JoinHandle<()>> = None;

    loop {
        let idle_deadline =
            idle_deadline(&inner, &last_activity, run_handle.is_some());

        tokio::select! {
            biased;

            Some(event) = event_rx.recv() => {
                last_activity = tokio::time::Instant::now();
                if let Err(e) = inner.persist_and_broadcast(&event).await {
                    tracing::error!(
                        session_id = %inner.session_id,
                        error = %e,
                        "Failed to persist or broadcast forwarded event"
                    );
                }
            }

            Some(op) = op_rx.recv() => {
                last_activity = tokio::time::Instant::now();
                match op {
                    SessionOp::Prompt { content, priority }
                    | SessionOp::Steer { content, priority } => {
                        if let Err(e) = inner.queue.push(
                            &inner.user_id,
                            &inner.session_id,
                            content,
                            priority,
                        ) {
                            tracing::error!(
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
                        if let Some(token) = inner.run_cancel.lock()
                            .expect("run_cancel mutex poisoned")
                            .as_ref()
                        {
                            token.cancel();
                        }
                        // Drop any queued inputs so the controller does not
                        // immediately restart the run after cancellation.
                        if let Err(e) = inner.queue.drain_pending(
                            &inner.user_id,
                            &inner.session_id,
                        ) {
                            tracing::error!(
                                session_id = %inner.session_id,
                                error = %e,
                                "Failed to drain pending inputs on cancel"
                            );
                        }
                        let mut state = inner.state.lock().expect("state mutex poisoned");
                        *state = SessionState::Cancelled;
                        if let Some(reason) = reason {
                            tracing::info!(
                                session_id = %inner.session_id,
                                reason,
                                "Session run cancelled"
                            );
                        }
                    }
                    SessionOp::Shutdown => {
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
                if inner.queue.has_pending(&inner.user_id, &inner.session_id)
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

fn idle_deadline(
    inner: &ControllerInner,
    last_activity: &tokio::time::Instant,
    run_active: bool,
) -> Option<tokio::time::Instant> {
    if run_active
        || inner.broadcaster.subscriber_count() > 0
        || inner.queue.has_pending(&inner.user_id, &inner.session_id)
    {
        None
    } else {
        Some(*last_activity + inner.idle_timeout)
    }
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

    use synthia_agent::{
        control::AgentRegistry,
        tools::orchestrator::build_default_tool_orchestrator,
        types::SessionEndReason,
    };
    use synthia_permission::HeadlessApprovalService;
    use synthia_sandbox::NoopSandboxManager;
    use synthia_session::store::EventStore;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;

    use super::*;

    fn test_deps(
        temp: &tempfile::TempDir,
        manager: &synthia_session::manager::SessionManager,
    ) -> RunDependencies {
        let workspace_root = temp.path().to_path_buf();
        let approval_service: Arc<dyn ApprovalService> =
            Arc::new(HeadlessApprovalService);
        let sandbox_manager: Arc<dyn SandboxManager> =
            Arc::new(NoopSandboxManager);
        let (tool_orchestrator, _tool_resolver) =
            build_default_tool_orchestrator(
                workspace_root.clone(),
                approval_service.clone(),
                sandbox_manager.clone(),
            );

        RunDependencies::new(
            Arc::new(test_support::FakeProvider::new(vec![])),
            Arc::new(RwLock::new(ToolRegistry::new())),
            manager.store().clone(),
            workspace_root,
            "fake-model".to_string(),
            Arc::new(crate::state::AppStateSubagentFactory::new(
                std::sync::Weak::new(),
            )) as Arc<dyn SubagentSessionFactory>,
            approval_service,
            sandbox_manager,
            tool_orchestrator,
            Arc::new(AgentControl::new(Arc::new(AgentRegistry::new()))),
            0,
        )
    }

    async fn make_manager_and_controller(
        idle_timeout: Duration,
        run_factory: Arc<dyn RunStreamFactory>,
    ) -> (
        Arc<SessionController>,
        synthia_session::manager::SessionManager,
        tempfile::TempDir,
    ) {
        let temp = tempfile::TempDir::new().unwrap();
        let manager = synthia_session::manager::SessionManager::new(
            temp.path().to_path_buf(),
        );
        manager
            .create_with_user("s1".to_string(), "alice".to_string())
            .await
            .unwrap();

        let deps = test_deps(&temp, &manager);
        let broadcaster = EventBroadcaster::new();
        let session_path = manager.store().session_dir("alice", "s1");
        let controller = SessionController::spawn(
            "alice",
            "s1",
            manager.input_queue(),
            session_path,
            broadcaster,
            deps,
            idle_timeout,
            run_factory,
            None,
        );

        (controller, manager, temp)
    }

    /// Create a child session under `parent_session_id` and spawn a
    /// controller wired to `parent_event_sender`.
    #[allow(clippy::too_many_arguments)]
    async fn make_child_controller(
        manager: &synthia_session::manager::SessionManager,
        temp: &tempfile::TempDir,
        user_id: &str,
        parent_session_id: &str,
        child_session_id: &str,
        idle_timeout: Duration,
        run_factory: Arc<dyn RunStreamFactory>,
        parent_event_sender: Option<mpsc::Sender<AgentEvent>>,
    ) -> Arc<SessionController> {
        manager
            .create_child(
                user_id.to_string(),
                parent_session_id.to_string(),
                Some(child_session_id.to_string()),
            )
            .await
            .unwrap();

        let deps = test_deps(temp, manager);
        let broadcaster = EventBroadcaster::new();
        let session_path =
            manager.store().session_dir(user_id, child_session_id);
        SessionController::spawn(
            user_id,
            child_session_id,
            manager.input_queue(),
            session_path,
            broadcaster,
            deps,
            idle_timeout,
            run_factory,
            parent_event_sender,
        )
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
        fn run_stream(&self, config: AgentRunConfig) -> AgentOutput {
            self.calls.lock().unwrap().push(config.clone());

            // Simulate the agent consuming pending inputs.
            if let Some(ref queue) = self.queue {
                let _ =
                    queue.drain_pending(&config.user_id, &config.session_id);
            }

            let events = self.events.clone();
            let (tx, rx) = mpsc::channel(events.len() + 1);
            tokio::spawn(async move {
                for ev in events {
                    if config.cancel_token.is_cancelled() {
                        break;
                    }
                    let _ = tx.send(ev).await;
                }
            });
            Box::pin(ReceiverStream::new(rx))
        }
    }

    /// A factory that blocks until its cancellation token fires,
    /// useful for verifying that only one run is active at a time.
    struct BlockingFactory {
        calls: Arc<Mutex<Vec<AgentRunConfig>>>,
        progress_count: Arc<AtomicUsize>,
    }

    impl RunStreamFactory for BlockingFactory {
        fn run_stream(&self, config: AgentRunConfig) -> AgentOutput {
            self.calls.lock().unwrap().push(config.clone());

            // Simulate the agent consuming pending inputs.
            if let Some(ref queue) = config.session_input_queue {
                let _ =
                    queue.drain_pending(&config.user_id, &config.session_id);
            }

            let token = config.cancel_token.clone();
            let count = Arc::clone(&self.progress_count);
            Box::pin(async_stream::stream! {
                loop {
                    if token.is_cancelled() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    count.fetch_add(1, Ordering::SeqCst);
                    yield AgentEvent::progress("working", count.load(Ordering::SeqCst), 0);
                }
                yield AgentEvent::SessionEnded {
                    reason: SessionEndReason::Cancelled,
                };
            })
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

    #[tokio::test]
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

        let pending =
            manager.input_queue().drain_pending("alice", "s1").unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].content, "turn left");
        assert_eq!(pending[0].priority, 200);
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
        let _ = manager.input_queue().drain_pending("alice", "s1");
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

    #[tokio::test]
    async fn test_events_are_persisted_and_broadcast() {
        let events = vec![
            AgentEvent::SessionStarted {
                session_id: "s1".to_string(),
            },
            AgentEvent::Finish {
                output: "done".to_string(),
            },
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
        let _ = manager.input_queue().drain_pending("alice", "s1");
        tokio::time::timeout(Duration::from_millis(500), async {
            while controller.state() != SessionState::Idle {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();

        let session_path = manager.store().session_dir("alice", "s1");
        let persisted =
            EventStore::new().read_from(&session_path, 0, 100).unwrap();
        assert!(!persisted.is_empty());
        assert!(persisted.iter().any(|e| e.event_type == "SessionStarted"));

        let received =
            tokio::time::timeout(Duration::from_millis(200), rx.recv())
                .await
                .unwrap()
                .unwrap();
        assert!(matches!(received, AgentEvent::SessionStarted { .. }));
    }

    #[tokio::test]
    async fn test_child_events_are_forwarded_to_parent() {
        let parent_calls = Arc::new(Mutex::new(Vec::new()));
        let parent_factory: Arc<dyn RunStreamFactory> =
            Arc::new(VecFactory::new(vec![], Arc::clone(&parent_calls), None));
        let (parent, manager, temp) = make_manager_and_controller(
            Duration::from_secs(60),
            parent_factory,
        )
        .await;

        let child_factory: Arc<dyn RunStreamFactory> = Arc::new(
            VecFactory::new(vec![], Arc::new(Mutex::new(Vec::new())), None),
        );
        let child = make_child_controller(
            &manager,
            &temp,
            "alice",
            "s1",
            "s1-child",
            Duration::from_secs(60),
            child_factory,
            Some(parent.event_sender()),
        )
        .await;

        let mut parent_rx = parent.subscribe();
        let mut child_rx = child.subscribe();

        let raw_event = AgentEvent::ToolCallStarted {
            tool_name: "read_file".to_string(),
            input: serde_json::json!({ "path": "/tmp/test" }),
        };
        child.event_sender().send(raw_event.clone()).await.unwrap();

        let received_child =
            tokio::time::timeout(Duration::from_millis(200), child_rx.recv())
                .await
                .unwrap()
                .unwrap();
        assert!(matches!(received_child, AgentEvent::ToolCallStarted { .. }));

        let received_parent =
            tokio::time::timeout(Duration::from_millis(200), parent_rx.recv())
                .await
                .unwrap()
                .unwrap();
        match received_parent {
            AgentEvent::SubagentEvent {
                child_session_id,
                event,
            } => {
                assert_eq!(child_session_id, "s1-child");
                assert!(matches!(
                    event.as_ref(),
                    AgentEvent::ToolCallStarted { .. }
                ));
            }
            other => panic!("expected SubagentEvent, got {other:?}"),
        }

        let parent_path = manager.store().session_dir("alice", "s1");
        let persisted =
            EventStore::new().read_from(&parent_path, 0, 100).unwrap();
        assert!(persisted.iter().any(|e| e.event_type == "subagent_event"));
    }

    #[tokio::test]
    async fn test_forwarding_survives_closed_parent_channel() {
        let parent_calls = Arc::new(Mutex::new(Vec::new()));
        let parent_factory: Arc<dyn RunStreamFactory> =
            Arc::new(VecFactory::new(vec![], Arc::clone(&parent_calls), None));
        let (parent, manager, temp) = make_manager_and_controller(
            Duration::from_secs(60),
            parent_factory,
        )
        .await;

        let parent_event_sender = parent.event_sender();
        parent.submit(SessionOp::Shutdown).await.unwrap();

        tokio::time::timeout(Duration::from_millis(500), async {
            while parent.is_alive() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();

        let child_factory: Arc<dyn RunStreamFactory> = Arc::new(
            VecFactory::new(vec![], Arc::new(Mutex::new(Vec::new())), None),
        );
        let child = make_child_controller(
            &manager,
            &temp,
            "alice",
            "s1",
            "s1-child-closed",
            Duration::from_secs(60),
            child_factory,
            Some(parent_event_sender),
        )
        .await;

        let mut child_rx = child.subscribe();
        let raw_event = AgentEvent::ToolCallStarted {
            tool_name: "read_file".to_string(),
            input: serde_json::json!({ "path": "/tmp/test" }),
        };
        child.event_sender().send(raw_event.clone()).await.unwrap();

        // The child must continue operating even though forwarding to
        // the dead parent fails.
        let received_child =
            tokio::time::timeout(Duration::from_millis(200), child_rx.recv())
                .await
                .unwrap()
                .unwrap();
        assert!(matches!(received_child, AgentEvent::ToolCallStarted { .. }));
    }

    /// `RunDependencies` must carry the configured `subagent_depth` so
    /// that `build_run_config` can call `manager.set_depth(...)`. This
    /// test pins the field through the constructor, complementing the
    /// existing `test_current_depth_returns_set_value` test in
    /// `synthia-agent` (which proves `set_depth` itself works).
    #[tokio::test]
    async fn run_dependencies_carries_subagent_depth() {
        let temp = tempfile::TempDir::new().unwrap();
        let manager = synthia_session::manager::SessionManager::new(
            temp.path().to_path_buf(),
        );
        manager
            .create_with_user("s1".to_string(), "alice".to_string())
            .await
            .unwrap();

        let workspace_root = temp.path().to_path_buf();
        let approval_service: Arc<dyn ApprovalService> =
            Arc::new(HeadlessApprovalService);
        let sandbox_manager: Arc<dyn SandboxManager> =
            Arc::new(NoopSandboxManager);
        let (tool_orchestrator, _tool_resolver) =
            build_default_tool_orchestrator(
                workspace_root.clone(),
                approval_service.clone(),
                sandbox_manager.clone(),
            );

        let deps = RunDependencies::new(
            Arc::new(test_support::FakeProvider::new(vec![])),
            Arc::new(RwLock::new(ToolRegistry::new())),
            manager.store().clone(),
            workspace_root,
            "fake-model".to_string(),
            Arc::new(crate::state::AppStateSubagentFactory::new(
                std::sync::Weak::new(),
            )) as Arc<dyn SubagentSessionFactory>,
            approval_service,
            sandbox_manager,
            tool_orchestrator,
            Arc::new(AgentControl::new(Arc::new(AgentRegistry::new()))),
            5,
        );
        assert_eq!(deps.subagent_depth, 5);
    }
}
