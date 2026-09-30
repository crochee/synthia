//! [`ReActAgent`] — the canonical [`Agent`] implementation's factory.
//!
//! This module owns the agent struct, the four constructors
//! (`new` / `with_options` / `with_prompt_context` / `with_descriptor`),
//! and the [`default_descriptor`] helper every constructor
//! delegates to. Every other concern lives in a focused sibling
//! module:
//!
//! | Module | Concern |
//! |---|---|
//! | [`super::agent_descriptor`] | `with_name` / `with_instructions` / `with_model_hint` / `with_output_schema` / `set_prompt_context` / `descriptor_mut` |
//! | [`super::agent_runtime`]    | `with_workspace` / `with_tool_registry` / `with_steering` / `with_context_manager` / `with_interceptor` / `with_max_iterations` / `with_tool_surface` / `with_tool_restriction` / `with_typed_event_sink` / `with_run_inbox` / `with_clock` / `with_spawner` / `with_strategy` / `with_compaction_settings` / `with_compaction_checkpoint` / `effective_context_manager` / `runtime_for_run` |
//! | [`super::agent_impl`]       | `impl Agent for ReActAgent` + `impl RegistryItem for ReActAgent` |
//!
//! The ReAct loop itself lives in [`super::loop_`]; the default
//! strategy in [`super::strategy`]; the system-prompt template
//! in [`super::prompt`]. The harness *is* the builder — every
//! `with_*` setter threads into this struct directly, no
//! separate factory type, no `build()`.
//!
//! [`Agent`]: crate::agent::Agent
//! [`RegistryItem`]: synthia_core::registry::RegistryItem

use std::{path::PathBuf, sync::Arc};

use synthia_context::{ContextManager, TruncatingContextManager};
use synthia_provider::traits::ModelProvider;
use synthia_session::TypedEventSink;
use synthia_steering::Steering;
use synthia_tool::{ToolRegistry, ToolRestriction, ToolSurfacePolicy};

use crate::{
    agent::{
        RunInbox,
        descriptor::AgentDescriptor,
        interceptor::ToolInterceptor,
        spawn::TokioSpawner,
        strategy::ReasoningStrategy,
    },
    prompt::PromptContext,
};

/// Concrete [`Agent`](crate::agent::Agent) implementing the ReAct loop.
///
/// Holds the shared dependencies (provider, tool registry) used by
/// every session. Each call to
/// [`Agent::run`](crate::agent::Agent::run) spawns a `ReActLoop`
/// bound to those shared deps.
pub struct ReActAgent {
    pub(super) descriptor: AgentDescriptor,
    pub(super) provider: Arc<dyn ModelProvider>,
    pub(super) tool_registry: Arc<ToolRegistry>,
    /// Deployment-level tool-surface policy (R34), or `None` for the
    /// unfiltered R33 projection. Stored behind an `Arc` so each
    /// dispatched loop shares one allocation.
    pub(super) tool_surface: Option<Arc<ToolSurfacePolicy>>,
    /// This agent's own allow/deny list (R58), or `None` for
    /// "every tool the surface advertises". Applied to the
    /// advertised list *and* enforced at dispatch.
    pub(super) tool_restriction: Option<Arc<ToolRestriction>>,
    /// Working directory handed to built-in tools as
    /// `Context::working_dir`.
    pub(super) workspace_root: PathBuf,
    /// Manifest context injected into the system prompt: tool
    /// definitions, enabled skills, and registered peer agents.
    /// See [`PromptContext`].
    pub(super) prompt_context: Arc<PromptContext>,
    /// Pluggable, async context-window manager. Default:
    /// [`TruncatingContextManager`] — pairwise-drop strategy that
    /// preserves the leading system prompt and evicts the oldest
    /// user/assistant pair until the message list fits in the
    /// resolved `ModelConfig::context_window`.
    pub(super) context_manager: Arc<dyn ContextManager>,
    /// Synthetic-tool plugins dispatched ahead of the registry (the
    /// `task` delegation seam lives in `synthia-tool-task`). Empty
    /// (the default) keeps the agent a single-loop harness with no
    /// delegation surface.
    pub(super) interceptors: Vec<Arc<dyn ToolInterceptor>>,
    /// Steering bundle: guards (pre-execution policy), hooks
    /// (async lifecycle), hints (advisory reminders), tracker
    /// (observation + concurrency recommendation), and the output
    /// transformer. Shared behind an `Arc` so every dispatched
    /// `ReActLoop` sees the same policy.
    pub(super) steering: Arc<Steering>,
    /// Per-run iteration cap. Defaults to [`DEFAULT_MAX_ITERATIONS`]
    /// when the agent is built via `new` / `with_options` /
    /// `with_descriptor`; overridable via
    /// [`with_max_iterations`](super::agent_runtime::ReActAgent::with_max_iterations).
    pub(super) max_iterations: usize,
    /// Typed-event sink for structural boundary events (R6-A).
    /// `None` disables typed emission. Set via
    /// [`with_typed_event_sink`](super::agent_runtime::ReActAgent::with_typed_event_sink).
    pub(super) typed_sink: Option<TypedEventSink>,
    /// Interactive message source for in-flight runs. `None` (the
    /// default) disables every inbox seam.
    pub(super) run_inbox: Option<Arc<dyn RunInbox>>,
    /// Clock used to stamp the runtime-context snapshot.
    pub(super) clock: synthia_core::SharedClock,
    /// Where a run's detached task goes. Default = [`TokioSpawner`].
    pub(super) spawner: Arc<dyn synthia_core::spawn::Spawner>,
    /// The reasoning loop. Default [`super::strategy::ReActStrategy`].
    pub(super) strategy: Arc<dyn ReasoningStrategy>,
    /// R13-3 / R34: LLM-backed compaction policy. `None` (the
    /// default) keeps the truncating context manager. When set,
    /// the context manager is upgraded to the provider-backed
    /// [`synthia_context::SummarizingContextManager`] (subject to
    /// the policy being enabled + valid). See
    /// [`with_compaction_settings`](super::agent_runtime::ReActAgent::with_compaction_settings).
    pub(super) compaction_settings: Option<synthia_context::CompactionSettings>,
    /// R34: externally owned compaction checkpoint (carries its
    /// own sink + ledger). When set, takes precedence over any
    /// auto-built checkpoint from
    /// [`with_compaction_settings`](super::agent_runtime::ReActAgent::with_compaction_settings).
    pub(super) compaction_checkpoint:
        Option<Arc<synthia_session::CompactionCheckpoint>>,
}

impl ReActAgent {
    pub fn new(
        provider: Arc<dyn ModelProvider>,
        tool_registry: Arc<ToolRegistry>,
    ) -> Self {
        Self::with_options(
            provider,
            tool_registry,
            PathBuf::new(),
            super::DEFAULT_SYSTEM_PROMPT.to_string(),
        )
    }

    /// Build a ReAct agent with full control over its prompt and
    /// working directory.
    pub fn with_options(
        provider: Arc<dyn ModelProvider>,
        tool_registry: Arc<ToolRegistry>,
        workspace_root: PathBuf,
        system_prompt: String,
    ) -> Self {
        let descriptor = default_descriptor(system_prompt);
        Self::with_descriptor(
            provider,
            tool_registry,
            workspace_root,
            descriptor,
            Arc::new(PromptContext::default()),
        )
    }

    /// Build a ReAct agent that injects the given prompt context
    /// (skills, peer agents, tool manifest) into the assembled
    /// system prompt.
    pub fn with_prompt_context(
        provider: Arc<dyn ModelProvider>,
        tool_registry: Arc<ToolRegistry>,
        workspace_root: PathBuf,
        system_prompt: String,
        prompt_context: Arc<PromptContext>,
    ) -> Self {
        Self::with_descriptor(
            provider,
            tool_registry,
            workspace_root,
            default_descriptor(system_prompt),
            prompt_context,
        )
    }

    /// Build a ReAct agent bound to an explicit [`AgentDescriptor`]
    /// and prompt manifest. Every other field starts at its
    /// "off" default and is composed in through the
    /// `with_*` setters.
    pub fn with_descriptor(
        provider: Arc<dyn ModelProvider>,
        tool_registry: Arc<ToolRegistry>,
        workspace_root: PathBuf,
        descriptor: AgentDescriptor,
        prompt_context: Arc<PromptContext>,
    ) -> Self {
        Self {
            descriptor,
            provider,
            tool_registry,
            tool_surface: None,
            tool_restriction: None,
            workspace_root,
            prompt_context,
            context_manager: Arc::new(TruncatingContextManager),
            interceptors: Vec::new(),
            steering: Arc::new(Steering::noop()),
            max_iterations: super::DEFAULT_MAX_ITERATIONS,
            typed_sink: None,
            run_inbox: None,
            clock: synthia_core::SharedClock::system(),
            spawner: Arc::new(TokioSpawner),
            strategy: Arc::new(super::strategy::ReActStrategy),
            compaction_settings: None,
            compaction_checkpoint: None,
        }
    }
}

/// The default `AgentDescriptor` a `new` / `with_options` /
/// `with_prompt_context` call installs. The descriptor is the
/// only piece of agent state these constructors used to inline
/// in literal form; centralising it in one helper keeps every
/// constructor below a single function call deep and gives
/// `with_descriptor` a clearly-stated contract: "caller supplies
/// the descriptor verbatim, here is what we would have built
/// for you otherwise".
fn default_descriptor(system_prompt: String) -> AgentDescriptor {
    AgentDescriptor {
        name: "agent".to_string(),
        // `display_name` is the human-readable label surfaced
        // to the model (in the `<identity>` block) and to
        // clients (on the `AgentCard`). Distinct from the
        // programmatic `name` ("agent"), which stays as the
        // registry / routing slug so existing user configs
        // and tests keep working unchanged.
        display_name: Some("Synthia".to_string()),
        description:
            "Default Synthia agent. Single-agent ReAct loop that uses \
             tools to complete user tasks end-to-end."
                .to_string(),
        kind: "react".to_string(),
        version: "1.0.0".to_string(),
        instructions: system_prompt,
        capabilities: vec![
            "tools".to_string(),
            "streaming".to_string(),
            "cancellation".to_string(),
        ],
        tools: Vec::new(),
        model_hint: None,
        handoffs: Vec::new(),
        handoff_hint: Some(
            "Use for general coding and tool-use tasks when no \
             specialist agent is a better fit."
                .to_string(),
        ),
        output_schema: None,
        owner: Some("synthia".to_string()),
        domain: Some("coding".to_string()),
        persona: Some(
            "You are Synthia, a pragmatic senior engineer \
             working alongside the user."
                .to_string(),
        ),
        max_iterations: None,
    }
}
