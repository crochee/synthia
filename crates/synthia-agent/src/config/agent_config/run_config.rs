//! Runtime configuration ([`AgentRunConfig`]) and its frozen snapshot
//! ([`AgentRunStateConfig`]) used for resume / fork operations.

use std::sync::Arc;

use synthia_context::{
    assembler::ContextAssembler,
    compaction::level1::CompactionProvider,
};
use synthia_core::tool::{
    extension_registry::ExtensionRegistry,
    rollout::RolloutTracker,
};
use synthia_hook::HookRegistry;
use synthia_memory::types::MemoryEvent;
use synthia_permission::ApprovalService;
use synthia_provider::{
    router::ModelRouter,
    traits::ModelProvider,
    types::Message,
};
use synthia_sandbox::SandboxManager;
use synthia_session::Store as SessionStore;
use synthia_tool::registry::ToolRegistry;
use synthia_tool_orchestrator::ToolOrchestrator;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::agent_config::AgentConfig;
use crate::{
    control::{AgentControl, fork_policy::ForkPolicy},
    input::AgentInput,
    interceptor::InterceptorChain,
    steering::SteeringChannel,
    subagent::SubagentSessionFactory,
};

/// Runtime agent configuration (in-memory, not serialised).
///
/// Holds the live provider, tool/hook registries, session store, and
/// cancellation primitives. Construct via [`super::AgentRunConfigBuilder`].
///
/// # Migration Note
///
/// Several fields overlap with [`crate::AgentHandle`]:
/// `provider`, `tool_registry`, `hook_registry`, `model_router`,
/// `context_assembler`, `session_store`, `approval_service`,
/// `sandbox_manager`, `memory_event_sender`.
///
/// New code should obtain these from `AgentHandle` instead of `AgentRunConfig`.
/// The simplified [`crate::RunConfig`] only carries runtime parameters
/// (`user_id`, `session_id`, `max_iterations`, `cancel_token`).
/// See the `synthia-agent-composition-a2a` OpenSpec change for details.
#[derive(Clone)]
pub struct AgentRunConfig {
    pub provider: Arc<dyn ModelProvider>,
    pub tool_registry: ToolRegistry,
    pub hook_registry: Arc<HookRegistry>,
    pub model_router: Arc<ModelRouter>,
    /// Owning user identifier. Required so that the on-disk session
    /// path, LLM provider prompt cache key, and tool permission
    /// decisions are all namespaced by user. See the
    /// `user-id-namespace-and-bash-permission-gate` OpenSpec change.
    pub user_id: String,
    pub session_id: String,
    pub input: AgentInput,
    pub config: AgentConfig,
    pub context_assembler: Option<Arc<ContextAssembler>>,
    pub session_store: SessionStore,
    pub steering_channel: Option<Arc<dyn SteeringChannel>>,
    pub session_input_queue: Option<synthia_session::store::SessionInputQueue>,
    pub cancel_token: CancellationToken,
    pub memory_event_sender: Option<mpsc::Sender<MemoryEvent>>,
    /// Optional control plane for multi-agent orchestration.
    pub agent_control: Option<AgentControl>,
    /// Policy governing what a forked sub-agent inherits.
    pub fork_policy: ForkPolicy,
    /// Optional L4 auto-compaction provider for the recovery cascade.
    ///
    /// When `Some(_)`, the `run_recovery_cascade` (L4 layer) can invoke
    /// `CompactionProvider::generate_summary` to reduce context
    /// utilization before falling through to L5 reset. When `None`, the
    /// cascade skips L4 and falls through to L5 (per
    /// `recovery_cascade::run_recovery_cascade` semantics).
    ///
    /// This is the *runtime* provider — distinct from
    /// [`AgentConfig::compaction_provider`], which is a static
    /// `ProviderConfig` used to serialize config to disk. Callers
    /// that hold a real LLM-backed `CompactionProvider` should set this
    /// field via [`super::AgentRunConfigBuilder::compaction_provider`].
    pub compaction_provider: Option<Arc<dyn CompactionProvider>>,
    /// Optional factory for creating real child sessions from agent-side
    /// tools such as the task tool. Injected by the server; `None` in
    /// standalone / REPL / test contexts.
    pub subagent_session_factory: Option<Arc<dyn SubagentSessionFactory>>,
    /// Optional approval service used by the tool orchestrator when a tool
    /// call requires explicit confirmation.
    pub approval_service: Option<Arc<dyn ApprovalService>>,
    /// Optional sandbox manager used by the tool orchestrator to select a
    /// sandbox profile before executing command-based tools.
    pub sandbox_manager: Option<Arc<dyn SandboxManager>>,
    /// Optional tool orchestrator that replaces direct `ToolRegistry`
    /// execution. When `Some`, the agent runtime routes tool calls through
    /// this orchestrator instead of `tool_registry`.
    pub tool_orchestrator: Option<Arc<dyn ToolOrchestrator>>,
    /// Optional extension manager for dynamic tool providers.
    /// When `None`, only static tools from `tool_registry` are available.
    pub extension_manager:
        Option<crate::tools::dynamic_provider::ExtensionManager>,
    /// Unified extension registry for the Registry-First architecture.
    ///
    /// Aggregates tool and fragment registries with shared lifecycle
    /// management. When `Some`, this becomes the primary interface for
    /// accessing extensions; when `None` (the default), the legacy
    /// per-field registries (`tool_registry`, etc.) are used instead.
    /// This enables progressive migration of individual fields into
    /// the unified registry without breaking existing code.
    pub extension_registry: Option<ExtensionRegistry>,
    /// Optional rollout tracker for tracking file changes and token
    /// usage during the agent loop. `None` = no rollout tracking.
    pub rollout_tracker: Option<Arc<RolloutTracker>>,
    /// Optional interceptor chain for cross-cutting concerns
    /// (permission, loop detection, approval, etc.).
    /// When `Some`, BeforeTool/AfterTool events are dispatched
    /// through the chain around tool execution. `None` = no
    /// interceptor dispatch (legacy behavior).
    pub interceptor_chain: Option<Arc<InterceptorChain>>,
    /// Optional [`GuardianCoordinator`] used as the permission gate before
    /// tool execution. `None` = Guardian disabled (legacy behavior). When
    /// `Some`, [`execute_and_emit`](crate::stream_builder::builder::tool_execution::execute::execute_and_emit)
    /// calls `GuardianCoordinator::check` for each tool call before
    /// delegating to `StepToolExecute::execute`.
    pub guardian_coordinator:
        Option<Arc<synthia_guardian::GuardianCoordinator>>,
    /// Cached loop services (bootstrap-once). Populated by
    /// [`LoopServices::bootstrap`] at the first call to
    /// `Agent::run_stream`. `None` until first access.
    pub loop_services: std::sync::OnceLock<crate::loop_services::LoopServices>,
}

/// Frozen runtime snapshot used for resume / fork.
///
/// Carries the [`AgentRunConfig`] alongside the initial messages and
/// the starting iteration so a paused agent can be resumed with
/// identical state.
pub struct AgentRunStateConfig {
    pub run_config: AgentRunConfig,
    pub initial_messages: Vec<Message>,
    pub start_iteration: usize,
}
