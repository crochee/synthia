//! [`AgentRuntime`] — every piece a [`ReasoningStrategy`](super::ReasoningStrategy)
//! needs to drive one session, pre-assembled by the owning agent.
//!
//! Kept in its own file because it is the largest concern in the
//! strategy seam and has nothing in common with the [`EventSink`]
//! it publishes through or with the [`ReasoningStrategy`](super::ReasoningStrategy)
//! trait itself. Splitting lets a reader open the file that
//! matches the question they came with.
//!
//! See [`super`] for the seam-level docs.

use std::{fmt, path::PathBuf, sync::Arc};

use synthia_context::ContextManager;
use synthia_core::{CancelToken, SharedClock, spawn::Spawner};
use synthia_provider::{ModelConfig, ModelProvider};
use synthia_steering::Steering;
use synthia_tool::{ToolRegistry, ToolSurfacePolicy};

use crate::{
    agent::{
        RunInbox,
        descriptor::AgentDescriptor,
        interceptor::ToolInterceptor,
    },
    prompt::PromptContext,
};

/// Everything a [`ReasoningStrategy`](super::ReasoningStrategy) needs to run one session.
///
/// Assembled per run by the agent that owns the pieces (see
/// [`crate::ReActAgent`]), so a strategy never has to know how the
/// agent was built — and a strategy that wants only the provider
/// can ignore the rest.
///
/// All fields are public: a consumer assembling an agent by hand (or
/// driving a strategy without an agent at all) constructs one directly.
#[derive(Clone)]
pub struct AgentRuntime {
    /// The model. The strategy owns the sampling loop, not the client.
    pub provider: Arc<dyn ModelProvider>,
    /// The tools the strategy *may* advertise. Whether it does is the
    /// strategy's decision — ReAct offers them, Chain-of-Thought does
    /// not.
    pub tool_registry: Arc<ToolRegistry>,
    /// Deployment-level tool-surface policy (`None` = the unfiltered
    /// projection).
    pub tool_surface: Option<Arc<ToolSurfacePolicy>>,
    /// R58: the per-agent allow/deny list, when the deployment (or the
    /// caller) configured one. Unlike [`Self::tool_surface`] — which is
    /// a *deployment* grouping/cap policy — this is the agent's own
    /// scope: the loop filters the advertised list by it **and refuses
    /// a denied call at dispatch**, so `denied_tools` is an enforced
    /// control rather than a hint. `None` means no restriction.
    pub tool_restriction: Option<Arc<synthia_tool::ToolRestriction>>,
    /// Working directory handed to built-in tools through the
    /// dispatch [`Context`](synthia_tool::Context).
    pub workspace_root: PathBuf,
    /// Identity + capability metadata: the system prompt's base
    /// instructions, the model hint, the agent's own cap.
    pub descriptor: AgentDescriptor,
    /// Skills / peer agents / tool manifest for the system prompt.
    pub prompt_context: Arc<PromptContext>,
    /// The context-window policy. A strategy SHOULD call
    /// [`ContextManager::prepare_arc`] before each model call — that
    /// is what makes a long conversation fit.
    pub context_manager: Arc<dyn ContextManager>,
    /// Synthetic-tool plugins, when the caller installed any (the
    /// delegation `task` seam lives in `synthia-tool-task`).
    pub interceptors: Vec<Arc<dyn ToolInterceptor>>,
    /// Guards, hooks, hints, tracker, output transformer. Consulted by
    /// the strategy at the seams it owns; a strategy with no tool loop
    /// simply has fewer seams.
    pub steering: Arc<Steering>,
    /// Per-run iteration cap from the agent (already clamped).
    pub max_iterations: usize,
    /// Structural event sink (iteration / step / usage boundaries).
    pub typed_sink: Option<synthia_session::TypedEventSink>,
    /// Interactive steering + follow-up source, when installed.
    pub inbox: Option<Arc<dyn RunInbox>>,
    /// The deployment's wall clock. Use it instead of `chrono::Utc::now()`.
    pub clock: SharedClock,
    /// Where to detach background work. Use it instead of `tokio::spawn`.
    pub spawner: Arc<dyn Spawner>,
    /// This run's cancellation token.
    pub cancel: Arc<dyn CancelToken>,
    /// Nesting depth: `0` for a top-level session, `depth + 1` for a
    /// child spawned by the `task` tool.
    pub subagent_depth: usize,
}

impl AgentRuntime {
    /// The resolved model configuration.
    #[must_use]
    pub fn model_config(&self) -> ModelConfig {
        self.provider.model_config()
    }

    /// The system prompt for this agent: base instructions plus the
    /// manifest (identity, skills, peer agents), rendered by the same
    /// deterministic assembler every strategy shares.
    ///
    /// Byte-stable for a given `(descriptor, prompt_context)` — the
    /// property provider prompt caching depends on — so a strategy
    /// computes it per run and reuses it across passes.
    #[must_use]
    pub fn system_prompt(&self) -> String {
        self.prompt_context.assemble(&self.descriptor)
    }
}

impl fmt::Debug for AgentRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AgentRuntime")
            .field("provider", &self.provider.name())
            .field("tools", &self.tool_registry.tool_count())
            .field("agent", &self.descriptor.name)
            .field("max_iterations", &self.max_iterations)
            .field("subagent_depth", &self.subagent_depth)
            .finish_non_exhaustive()
    }
}

/// Build a minimally-populated [`AgentRuntime`] for in-crate tests.
///
/// Three call sites (one per shipped strategy: `strategy.rs`,
/// `cot.rs`, `best_of_n.rs`) used to build the 22-field literal by
/// hand. Centralising it here keeps the test intent ("which two
/// fields does this test care about?") visible at the call site
/// while removing the boilerplate.
#[cfg(test)]
pub(crate) fn default_for_test(
    provider: Arc<dyn ModelProvider>,
    cancel: Arc<dyn CancelToken>,
) -> AgentRuntime {
    AgentRuntime {
        provider,
        tool_registry: Arc::new(ToolRegistry::new()),
        tool_surface: None,
        tool_restriction: None,
        workspace_root: PathBuf::from("."),
        descriptor: AgentDescriptor {
            name: "test".into(),
            description: String::new(),
            kind: "test".into(),
            version: "0.1.0".into(),
            instructions: "be brief".into(),
            capabilities: Vec::new(),
            tools: Vec::new(),
            handoffs: Vec::new(),
            handoff_hint: None,
            model_hint: None,
            output_schema: None,
            owner: None,
            domain: None,
            persona: None,
            max_iterations: None,
            display_name: None,
        },
        prompt_context: Arc::new(PromptContext::default()),
        context_manager: Arc::new(synthia_context::TruncatingContextManager),
        interceptors: Vec::new(),
        steering: Arc::new(Steering::noop()),
        max_iterations: 4,
        typed_sink: None,
        inbox: None,
        clock: SharedClock::system(),
        spawner: Arc::new(crate::agent::spawn::TokioSpawner),
        cancel,
        subagent_depth: 0,
    }
}
