//! Builder for [`AgentRunConfig`].
//!
//! Unlike [`super::AgentConfigBuilder`], this builder is **strict**:
//! `user_id` is a required field and `build()` returns an error if it
//! is missing or empty. This is the runtime counterpart to the
//! on-disk user_id-namespace gate.

use std::sync::Arc;

use synthia_context::{
    assembler::ContextAssembler,
    compaction::level1::CompactionProvider,
};
use synthia_core::Error;
use synthia_hook::HookRegistry;
use synthia_memory::types::MemoryEvent;
use synthia_permission::ApprovalService;
use synthia_provider::{router::ModelRouter, traits::ModelProvider};
use synthia_sandbox::SandboxManager;
use synthia_session::{Store as SessionStore, store::SessionInputQueue};
use synthia_tool::registry::ToolRegistry;
use synthia_tool_orchestrator::ToolOrchestrator;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{agent_config::AgentConfig, run_config::AgentRunConfig};
use crate::{
    control::{AgentControl, fork_policy::ForkPolicy},
    input::AgentInput,
    steering::SteeringChannel,
    subagent::SubagentSessionFactory,
};

#[derive(Default)]
pub struct AgentRunConfigBuilder {
    provider: Option<Arc<dyn ModelProvider>>,
    tool_registry: Option<ToolRegistry>,
    hook_registry: Option<Arc<HookRegistry>>,
    model_router: Option<Arc<ModelRouter>>,
    user_id: Option<String>,
    session_id: Option<String>,
    input: Option<AgentInput>,
    config: Option<AgentConfig>,
    context_assembler: Option<Arc<ContextAssembler>>,
    session_store: Option<SessionStore>,
    steering_channel: Option<Arc<dyn SteeringChannel>>,
    session_input_queue: Option<SessionInputQueue>,
    cancel_token: Option<CancellationToken>,
    memory_event_sender: Option<mpsc::Sender<MemoryEvent>>,
    agent_control: Option<AgentControl>,
    fork_policy: Option<ForkPolicy>,
    compaction_provider: Option<Arc<dyn CompactionProvider>>,
    subagent_session_factory: Option<Arc<dyn SubagentSessionFactory>>,
    approval_service: Option<Arc<dyn ApprovalService>>,
    sandbox_manager: Option<Arc<dyn SandboxManager>>,
    tool_orchestrator: Option<Arc<dyn ToolOrchestrator>>,
    guardian_coordinator: Option<Arc<synthia_guardian::GuardianCoordinator>>,
}

impl AgentRunConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn provider(mut self, provider: Arc<dyn ModelProvider>) -> Self {
        self.provider = Some(provider);
        self
    }

    pub fn tool_registry(mut self, registry: ToolRegistry) -> Self {
        self.tool_registry = Some(registry);
        self
    }

    pub fn hook_registry(mut self, hooks: Arc<HookRegistry>) -> Self {
        self.hook_registry = Some(hooks);
        self
    }

    pub fn model_router(mut self, router: Arc<ModelRouter>) -> Self {
        self.model_router = Some(router);
        self
    }

    /// Owning user identifier. Required: a non-empty `user_id` is
    /// enforced by [`build`] so the on-disk session path, LLM provider
    /// prompt cache key, and tool permission decisions are all
    /// namespaced. Sessions created with the legacy single-tenant
    /// layout can be promoted later via
    /// [`SessionManager::assign_user`].
    pub fn user_id(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn session_id(mut self, id: String) -> Self {
        self.session_id = Some(id);
        self
    }

    pub fn input(mut self, input: AgentInput) -> Self {
        self.input = Some(input);
        self
    }

    pub fn config(mut self, config: AgentConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn context_assembler(
        mut self,
        assembler: Arc<ContextAssembler>,
    ) -> Self {
        self.context_assembler = Some(assembler);
        self
    }

    pub fn session_store(mut self, store: SessionStore) -> Self {
        self.session_store = Some(store);
        self
    }

    pub fn steering_channel(
        mut self,
        channel: Arc<dyn SteeringChannel>,
    ) -> Self {
        self.steering_channel = Some(channel);
        self
    }

    pub fn session_input_queue(mut self, queue: SessionInputQueue) -> Self {
        self.session_input_queue = Some(queue);
        self
    }

    pub fn cancel_token(mut self, token: CancellationToken) -> Self {
        self.cancel_token = Some(token);
        self
    }

    pub fn memory_event_sender(
        mut self,
        sender: mpsc::Sender<MemoryEvent>,
    ) -> Self {
        self.memory_event_sender = Some(sender);
        self
    }

    pub fn agent_control(mut self, control: AgentControl) -> Self {
        self.agent_control = Some(control);
        self
    }

    pub fn fork_policy(mut self, policy: ForkPolicy) -> Self {
        self.fork_policy = Some(policy);
        self
    }

    /// Inject an L4 auto-compaction provider used by the recovery
    /// cascade. When `None` (the default), the cascade skips L4 and
    /// falls through to L5 reset. See
    /// [`AgentRunConfig::compaction_provider`].
    pub fn compaction_provider(
        mut self,
        provider: Arc<dyn CompactionProvider>,
    ) -> Self {
        self.compaction_provider = Some(provider);
        self
    }

    /// Inject a factory for creating real child sessions from agent-side
    /// tools. Server-side callers should provide the implementation
    /// backed by the server's session manager.
    pub fn subagent_session_factory(
        mut self,
        factory: Arc<dyn SubagentSessionFactory>,
    ) -> Self {
        self.subagent_session_factory = Some(factory);
        self
    }

    /// Inject the approval service used by the tool orchestrator.
    pub fn approval_service(
        mut self,
        service: Arc<dyn ApprovalService>,
    ) -> Self {
        self.approval_service = Some(service);
        self
    }

    /// Inject the sandbox manager used by the tool orchestrator.
    pub fn sandbox_manager(mut self, manager: Arc<dyn SandboxManager>) -> Self {
        self.sandbox_manager = Some(manager);
        self
    }

    /// Inject the tool orchestrator that replaces direct `ToolRegistry`
    /// execution.
    pub fn tool_orchestrator(
        mut self,
        orchestrator: Arc<dyn ToolOrchestrator>,
    ) -> Self {
        self.tool_orchestrator = Some(orchestrator);
        self
    }

    /// Inject the [`GuardianCoordinator`] used as the permission gate
    /// before tool execution. `None` (the default) disables Guardian and
    /// preserves legacy behavior.
    pub fn guardian_coordinator(
        mut self,
        coordinator: Arc<synthia_guardian::GuardianCoordinator>,
    ) -> Self {
        self.guardian_coordinator = Some(coordinator);
        self
    }

    pub fn build(self) -> Result<AgentRunConfig, Error> {
        let user_id = self.user_id.ok_or_else(|| {
            Error::Validation("missing required field: user_id".into())
        })?;
        if user_id.is_empty() {
            return Err(Error::Validation(
                "user_id must be a non-empty string".into(),
            ));
        }
        Ok(AgentRunConfig {
            provider: self.provider.ok_or_else(|| {
                Error::Validation("missing required field: provider".into())
            })?,
            tool_registry: self.tool_registry.ok_or_else(|| {
                Error::Validation(
                    "missing required field: tool_registry".into(),
                )
            })?,
            hook_registry: self.hook_registry.ok_or_else(|| {
                Error::Validation(
                    "missing required field: hook_registry".into(),
                )
            })?,
            model_router: self.model_router.ok_or_else(|| {
                Error::Validation("missing required field: model_router".into())
            })?,
            user_id,
            session_id: self.session_id.ok_or_else(|| {
                Error::Validation("missing required field: session_id".into())
            })?,
            input: self.input.ok_or_else(|| {
                Error::Validation("missing required field: input".into())
            })?,
            config: self.config.ok_or_else(|| {
                Error::Validation("missing required field: config".into())
            })?,
            context_assembler: self.context_assembler,
            session_store: self.session_store.ok_or_else(|| {
                Error::Validation(
                    "missing required field: session_store".into(),
                )
            })?,
            steering_channel: self.steering_channel,
            session_input_queue: self.session_input_queue,
            cancel_token: self.cancel_token.ok_or_else(|| {
                Error::Validation("missing required field: cancel_token".into())
            })?,
            memory_event_sender: self.memory_event_sender,
            agent_control: self.agent_control,
            fork_policy: self.fork_policy.unwrap_or_default(),
            compaction_provider: self.compaction_provider,
            subagent_session_factory: self.subagent_session_factory,
            approval_service: self.approval_service,
            sandbox_manager: self.sandbox_manager,
            tool_orchestrator: self.tool_orchestrator,
            guardian_coordinator: self.guardian_coordinator,
        })
    }
}
