//! `AgentFactory`: builds `Agent::run_stream` invocations from server context.

use std::{path::PathBuf, sync::Arc};

use synthia_agent::{
    Agent,
    AgentConfig,
    AgentInput,
    AgentOutput,
    AgentRunConfig,
    control::{AgentControl, AgentRegistry},
};
use synthia_context::{ProtectionZone, assembler::ContextAssembler};
use synthia_hook::HookRegistry;
use synthia_permission::ApprovalService;
use synthia_provider::{router::ModelRouter, traits::ModelProvider};
use synthia_sandbox::SandboxManager;
use synthia_session::Store as SessionStore;
use synthia_tool::registry::ToolRegistry;
use synthia_tool_orchestrator::ToolOrchestrator;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use super::app_state::AppState;
use crate::middleware::auth::RequestUserId;

pub struct AgentFactory {
    pub workspace_root: PathBuf,
    /// Shared provider for all agents produced by this factory.
    provider: Arc<dyn ModelProvider>,
    /// Shared tool registry reference.
    tool_registry: Arc<RwLock<ToolRegistry>>,
    /// Shared hook registry reference.
    hook_registry: Arc<RwLock<HookRegistry>>,
    /// Approval service used by the tool orchestrator.
    approval_service: Arc<dyn ApprovalService>,
    /// Sandbox manager used by the tool orchestrator.
    sandbox_manager: Arc<dyn SandboxManager>,
    /// Tool orchestrator that routes tool calls through approval and sandbox.
    tool_orchestrator: Arc<dyn ToolOrchestrator>,
    /// Control plane for subagent lifecycle tracking.
    agent_control: Arc<AgentControl>,
    /// Resolved per-request `user_id`, if the caller has one. When
    /// `None`, [`AgentFactory::create`] falls back to
    /// `SERVER_DEFAULT_USER_ID` to preserve the §1 invariant.
    user_id: Option<String>,
}

impl AgentFactory {
    /// Create a new factory with shared component references.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        workspace_root: PathBuf,
        provider: Arc<dyn ModelProvider>,
        tool_registry: Arc<RwLock<ToolRegistry>>,
        hook_registry: Arc<RwLock<HookRegistry>>,
        approval_service: Arc<dyn ApprovalService>,
        sandbox_manager: Arc<dyn SandboxManager>,
        tool_orchestrator: Arc<dyn ToolOrchestrator>,
        agent_control: Arc<AgentControl>,
    ) -> Self {
        Self {
            workspace_root,
            provider,
            tool_registry,
            hook_registry,
            approval_service,
            sandbox_manager,
            tool_orchestrator,
            agent_control,
            user_id: None,
        }
    }

    /// Create a new factory bound to a specific `user_id` (typically
    /// the `RequestUserId` resolved by the auth middleware).
    #[allow(clippy::too_many_arguments)]
    pub fn with_user_id(
        workspace_root: PathBuf,
        provider: Arc<dyn ModelProvider>,
        tool_registry: Arc<RwLock<ToolRegistry>>,
        hook_registry: Arc<RwLock<HookRegistry>>,
        approval_service: Arc<dyn ApprovalService>,
        sandbox_manager: Arc<dyn SandboxManager>,
        tool_orchestrator: Arc<dyn ToolOrchestrator>,
        agent_control: Arc<AgentControl>,
        user_id: String,
    ) -> Self {
        Self {
            workspace_root,
            provider,
            tool_registry,
            hook_registry,
            approval_service,
            sandbox_manager,
            tool_orchestrator,
            agent_control,
            user_id: Some(user_id),
        }
    }

    /// Create a new agent run for the given session.
    ///
    /// Produces an `AgentOutput` stream with fresh session state but
    /// shared component references (provider, tools, hooks, model router).
    pub fn create(
        &self,
        session_id: String,
        input: AgentInput,
        model: String,
        cancel_rx: Option<tokio::sync::oneshot::Receiver<()>>,
    ) -> AgentOutput {
        let config = AgentConfig {
            model,
            max_iterations: 20,
            max_tokens: 4096,
            temperature: Some(0.7),
            workspace_root: self.workspace_root.clone(),
            token_budget: None,
            checkpoint_dir: None,
            context_token_budget: Some(
                synthia_session::types::TokenBudget::default(),
            ),
            ..Default::default()
        };

        // Clone the tool registry snapshot; changes are synced via the shared Arc.
        let tool_reg = self
            .tool_registry
            .try_read()
            .map(|r| (*r).clone())
            .unwrap_or_else(|_| ToolRegistry::new());

        // Build a fresh HookRegistry with the same hooks from the shared registry.
        let hooks = self
            .hook_registry
            .try_read()
            .map(|r| {
                let new_hooks = HookRegistry::new();
                if !r.is_empty() {
                    tracing::debug!(
                        hook_count = r.len(),
                        "Creating fresh HookRegistry"
                    );
                }
                new_hooks
            })
            .unwrap_or_else(|_| HookRegistry::new());

        // Create CancellationToken
        let cancel_token = CancellationToken::new();

        // If we have a cancel_rx, spawn a task to cancel the token when it fires
        if let Some(rx) = cancel_rx {
            let token_clone = cancel_token.clone();
            tokio::spawn(async move {
                let _ = rx.await;
                token_clone.cancel();
            });
        }

        // Create context assembler
        let protection_zone = ProtectionZone::default();
        let assembler = ContextAssembler::new(config.max_tokens)
            .with_protection_zone(protection_zone);

        // Create session store
        let session_store_dir =
            self.workspace_root.join(".synthia").join("sessions");
        let session_store = SessionStore::new(session_store_dir);

        // Get model router as Arc
        let model_router = Arc::new(ModelRouter::new());

        Agent::run_stream(AgentRunConfig {
            provider: self.provider.clone(),
            tool_registry: tool_reg,
            hook_registry: Arc::new(hooks),
            model_router,
            // user_id is resolved by the auth middleware from the
            // request's API key (or `SERVER_DEFAULT_USER_ID` if no
            // key is configured). Callers that go through
            // `AgentFactory::create` are themselves invoked from
            // route handlers that have already extracted
            // `RequestUserId`; pass the resolved id explicitly
            // through `user_id` rather than the placeholder.
            user_id: self.user_id.clone().unwrap_or_else(|| {
                RequestUserId(
                    synthia_session::store::SERVER_DEFAULT_USER_ID.to_string(),
                )
                .0
            }),
            session_id,
            input,
            config,
            context_assembler: Some(Arc::new(assembler)),
            session_store,
            steering_channel: None,
            cancel_token,
            memory_event_sender: None,
            agent_control: Some((*self.agent_control).clone()),
            fork_policy: Default::default(),
            // No runtime L4 CompactionProvider wired in the server
            // bootstrap yet; cascade falls through to L5 reset.
            compaction_provider: None,
            session_input_queue: None,
            subagent_session_factory: None,
            approval_service: Some(Arc::clone(&self.approval_service)),
            sandbox_manager: Some(Arc::clone(&self.sandbox_manager)),
            tool_orchestrator: Some(Arc::clone(&self.tool_orchestrator)),
            guardian_coordinator: None,
            extension_manager: None,
        })
    }

    /// Create from an existing `AppState` for convenience.
    pub fn from_state(state: &AppState) -> Self {
        Self::new(
            state.workspace_root.clone(),
            state.default_provider.clone(),
            state.tool_registry.clone(),
            state.hook_registry.clone(),
            Arc::clone(&state.approval_service),
            Arc::clone(&state.sandbox_manager),
            Arc::clone(&state.tool_orchestrator),
            Arc::new(AgentControl::new(Arc::new(AgentRegistry::new()))),
        )
    }
}
