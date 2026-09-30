//! What a run needs: the provider, tool registry, prompt context and
//! per-agent knobs a controller hands to every agent run it starts.

use std::{path::PathBuf, sync::Arc};

use synthia::{
    harness::{CompactionEmitters, PromptContext},
    provider::traits::ModelProvider,
    tool::registry::ToolRegistry,
};
use tokio::sync::RwLock;

use crate::state::UsageMetrics;

/// Minimal dependencies required to build an
/// [`AgentRunConfig`](synthia::harness::AgentRunConfig) for
/// the session.
#[derive(Clone)]
pub struct RunDependencies {
    pub provider: Arc<dyn ModelProvider>,
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
    /// Working directory handed to built-in tools via
    /// [`AgentRunConfig::workspace_root`](synthia::harness::AgentRunConfig::workspace_root). Replaces the
    /// previous hard-coded `/tmp` so `read_file` / `shell`
    /// operate inside the user's project root, not the
    /// system temp dir.
    pub workspace_root: PathBuf,
    /// System prompt injected as the first message of every
    /// conversation. The ReAct loop builds its own system
    /// prompt from the descriptor via
    /// [`synthia::harness::PromptContext::assemble`]; this
    /// field is the legacy/default fallback for callers that
    /// do not supply an explicit descriptor.
    pub system_prompt: String,
    /// Prompt-context manifest (skills + peer agents + tool
    /// definitions). Empty by default; populated by the server
    /// from the workspace's `.agents/skills/` directory and the
    /// registered [`AgentRegistry`](synthia::harness::AgentRegistry) at startup.
    ///
    /// Stored as `Arc<PromptContext>` so cloning the
    /// [`RunDependencies`] (which the [`crate::state::AppState`]
    /// does on every session creation) bumps a refcount instead
    /// of deep-cloning the skills + peer-agents lists. The
    /// manifest is read-only after boot, so this is safe across
    /// concurrent dispatches.
    pub prompt_context: Arc<synthia::harness::PromptContext>,
    /// Steering bundle (guards / hooks / hints / tracker)
    /// installed on every agent built by the run factory.
    /// Defaults to `Steering::noop()`; the AppState wires
    /// `Steering::default_policy(workspace_root)` at boot.
    pub steering: Arc<synthia::steering::Steering>,
    /// Multi-agent registry. Held by reference so the run
    /// factory can resolve the configured agent synchronously
    /// inside `build_run_config` without going through an
    /// async dispatch boundary.
    pub agent_registry: Option<Arc<synthia::harness::AgentRegistry>>,
    pub default_agent_name: Option<Arc<parking_lot::RwLock<Option<String>>>>,
    /// Wall-clock source for the session log's `ts` fields and for
    /// the operation snapshots this deployment publishes. Default
    /// [`synthia::core::SystemClock`]; the server passes the single
    /// process clock it builds at boot, and a test injects a
    /// [`synthia::core::FixedClock`] via [`RunDependencies::with_clock`].
    pub clock: synthia::core::SharedClock,
    /// Default per-run iteration cap applied to every dispatch
    /// unless a per-run override is supplied via
    /// [`RunDependencies::with_max_iterations`]. Sourced from
    /// `agents.<name>.max_steps` at boot and forward to the
    /// factory, which installs it via
    /// [`synthia::harness::ReActAgent::with_max_iterations`] so
    /// the loop owns a stable cap for the whole run.
    pub default_max_iterations: usize,
    /// R16: optional LLM-backed compaction policy. Loaded from
    /// `agents.<name>.compaction` in the server config; composed
    /// into the run's context manager.
    pub compaction: Option<synthia::context::CompactionSettings>,
    /// R34: deployment-level tool-surface policy installed on every
    /// agent the run factory builds. `None` (the default) keeps the
    /// unfiltered R33 projection. Loaded from the `[tools]` config
    /// section at boot.
    pub tool_surface: Option<synthia::tool::ToolSurfacePolicy>,
    /// R50: deployment-level reasoning loop installed on every agent
    /// the run factory builds. `None` (the default) keeps ReAct.
    ///
    /// Resolved once at boot from `agents.<default>.strategy` rather
    /// than per dispatch: a strategy instance is immutable and
    /// shareable, so every run clones one `Arc` and the config file is
    /// read exactly once.
    pub strategy: Option<Arc<dyn synthia::harness::agent::ReasoningStrategy>>,
    /// R58: the agent's own allow/deny list, resolved at boot from
    /// `agents.<default>.{allowed_tools,denied_tools}`. `None` (the
    /// default) leaves every advertised tool callable.
    pub tool_restriction: Option<Arc<synthia::tool::ToolRestriction>>,
    /// Process-wide usage counters the controller's event funnel
    /// records into (token totals per `SystemEvent::Usage`, one turn
    /// per `SystemEvent::SessionEnded`). Defaults to a private
    /// [`UsageMetrics`], which keeps a run built without an
    /// `AppState` (tests, embedders) compiling and counting into a
    /// sink nobody reads; the server installs the shared one via
    pub usage_metrics: Arc<UsageMetrics>,
    /// Sub-agent session router. `None` (tests, embedders) drops
    /// delegated child events instead of inlining them into the
    /// parent — the child's answer still reaches the model through
    /// the `task` tool result. The server installs the process-wide
    /// router so every child gets a first-class session.
    pub subagent_router:
        Option<Arc<crate::session::subagent_router::SubagentRouter>>,
}

impl RunDependencies {
    /// Builder: install the sub-agent session router.
    #[must_use]
    pub fn with_subagent_router(
        mut self,
        router: Option<Arc<crate::session::subagent_router::SubagentRouter>>,
    ) -> Self {
        self.subagent_router = router;
        self
    }

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
            steering: Arc::new(synthia::steering::Steering::noop()),
            prompt_context: Arc::new(PromptContext::default()),
            agent_registry: None,
            default_agent_name: None,
            clock: synthia::core::SharedClock::system(),
            default_max_iterations: synthia::harness::DEFAULT_MAX_ITERATIONS,
            compaction: None,
            tool_surface: None,
            strategy: None,
            tool_restriction: None,
            usage_metrics: Arc::new(UsageMetrics::default()),
            subagent_router: None,
        }
    }

    /// Install the wall-clock source this session's log rows are
    /// stamped from.
    #[must_use]
    pub fn with_clock(mut self, clock: synthia::core::SharedClock) -> Self {
        self.clock = clock;
        self
    }

    /// R16: install the LLM-backed compaction policy. The run
    /// factory upgrades the context manager when this is set.
    #[must_use]
    pub fn with_compaction(
        mut self,
        compaction: synthia::context::CompactionSettings,
    ) -> Self {
        self.compaction = Some(compaction);
        self
    }

    /// R16: `Option`-flavoured `with_compaction` for callers
    /// that thread an already-optional policy through.
    #[must_use]
    pub fn with_optional_compaction(
        mut self,
        compaction: Option<synthia::context::CompactionSettings>,
    ) -> Self {
        self.compaction = compaction;
        self
    }

    /// R34: `Option`-flavoured installer for the deployment's
    /// tool-surface policy. `None` is a no-op (the agent keeps the
    /// unfiltered R33 projection), which is what a deployment with no
    /// `[tools]` section gets.
    #[must_use]
    pub fn with_optional_tool_surface(
        mut self,
        tool_surface: Option<synthia::tool::ToolSurfacePolicy>,
    ) -> Self {
        self.tool_surface = tool_surface;
        self
    }

    /// R50: `Option`-flavoured installer for the deployment's
    /// reasoning loop. `None` is a no-op (the agent keeps
    /// [`ReActStrategy`](synthia::harness::agent::ReActStrategy)), which
    /// is what a deployment with no `strategy =` in its agent config
    /// gets.
    #[must_use]
    pub fn with_optional_strategy(
        mut self,
        strategy: Option<Arc<dyn synthia::harness::agent::ReasoningStrategy>>,
    ) -> Self {
        self.strategy = strategy;
        self
    }

    /// R58: `Option`-flavoured installer for the agent's allow/deny
    /// list. `None` is a no-op (every advertised tool stays callable).
    #[must_use]
    pub fn with_optional_tool_restriction(
        mut self,
        restriction: Option<Arc<synthia::tool::ToolRestriction>>,
    ) -> Self {
        self.tool_restriction = restriction;
        self
    }

    /// Install the process-wide usage counters this session's event
    /// funnel records into. `AppState` shares one `Arc` across every
    /// controller it spawns, so a closed session's tokens still count
    /// toward the process totals.
    #[must_use]
    pub fn with_usage_metrics(mut self, usage: Arc<UsageMetrics>) -> Self {
        self.usage_metrics = usage;
        self
    }

    /// R22: compose the run's context manager from the configured
    /// capabilities.
    ///
    /// Order matters: the grounded manager injects retrieved
    /// context and then delegates inward, so compaction still
    /// trims to the window after grounding. With neither
    /// capability configured this returns `None`, and the agent
    /// keeps its default (truncating) manager.
    #[must_use]
    pub fn compose_context_manager(
        &self,
        provider: Arc<dyn ModelProvider>,
    ) -> Option<Arc<dyn synthia::context::ContextManager>> {
        self.compose_context_manager_with_emitters(provider, None)
    }

    /// R34: [`Self::compose_context_manager`] with the durable
    /// compaction emitters installed on the summarising manager.
    ///
    /// The emitters are reached only when the deployment actually
    /// configures compaction; without a policy they are dropped, the
    /// same way the policy-less path keeps the loop's default
    /// manager.
    #[must_use]
    pub fn compose_context_manager_with_emitters(
        &self,
        provider: Arc<dyn ModelProvider>,
        emitters: Option<CompactionEmitters>,
    ) -> Option<Arc<dyn synthia::context::ContextManager>> {
        self.compaction.map(|settings| {
            synthia::harness::context_manager_for_compaction_with_emitters(
                provider, settings, emitters,
            )
        })
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

    /// Install the steering policy used by every run built from
    /// these dependencies.
    pub fn with_steering(
        mut self,
        steering: Arc<synthia::steering::Steering>,
    ) -> Self {
        self.steering = steering;
        self
    }

    /// Override the per-run iteration cap. Useful for tests and
    /// for callers that want a different cap than the AppState
    /// default (e.g. a chat request pinning `max_steps` via
    /// a future API parameter). The agent-side
    /// `[1, 4096]` clamp still applies.
    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.default_max_iterations = max_iterations.clamp(1, 4096);
        self
    }

    /// Wire the multi-agent registry + configured default so
    /// the run factory can resolve the configured agent
    /// synchronously inside `build_run_config`.
    pub fn with_agent_registry(
        mut self,
        registry: Arc<synthia::harness::AgentRegistry>,
        default_agent_name: Arc<parking_lot::RwLock<Option<String>>>,
    ) -> Self {
        self.agent_registry = Some(registry);
        self.default_agent_name = Some(default_agent_name);
        self
    }
}
