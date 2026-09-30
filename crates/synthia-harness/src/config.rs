//! The per-session run configuration.
//!
//! [`AgentRunConfig`] is the bag a host (the server's `SessionController`,
//! or any consumer that dispatches sessions) hands to a run factory.
//!
//! What a run cannot start without: `provider`, `tool_registry`,
//! `workspace_root`, `system_prompt`, `prompt_context`, and `steering`.
//! Every other field is an `Option` — `agent_resolver`, `agent_name`,
//! `max_iterations`, `agent_registry`, `typed_event_sink`,
//! `context_manager`, `tool_surface`, `strategy`, `tool_restriction` —
//! so a consumer supplies only the capabilities it actually composes.
//!
//! One type rather than a dozen parameters because a run factory threads
//! the same bundle through every dispatch.
//!
//! The module is private; [`AgentRunConfig`] is re-exported at the crate
//! root, which is the only path a consumer needs.

use std::{path::PathBuf, sync::Arc};

use synthia_provider::traits::ModelProvider;
use synthia_steering::Steering;
use synthia_tool::registry::ToolRegistry;

use crate::{agent::ReasoningStrategy, prompt::PromptContext};

/// Per-session configuration consumed by the run factory inside
/// `SessionController` — provider, tool registry, workspace, system
/// prompt, the per-session manifest and the optional sync resolver
/// used to bind a named agent to its [`AgentDescriptor`].
///
/// [`AgentDescriptor`]: crate::agent::AgentDescriptor
#[derive(Clone)]
pub struct AgentRunConfig {
    /// The model backing the run.
    pub provider: Arc<dyn ModelProvider>,
    /// The tools the run may advertise and dispatch.
    pub tool_registry: Arc<ToolRegistry>,
    /// Absolute working directory passed to built-in tools
    /// (`read_file`, `read`, `shell`, …) via
    /// [`synthia_tool::Context`]. Replaces the legacy empty
    /// path so `read_file` / `shell` operate inside the user's
    /// project root, not the system temp dir.
    pub workspace_root: PathBuf,
    /// System prompt injected as the first message of every
    /// conversation. Used as the base instructions when the
    /// descriptor resolver returns `None` (legacy single-agent
    /// path).
    pub system_prompt: String,
    /// Manifest context (skills + peer agents + tool manifest)
    /// the [`PromptContext`] assembler renders into the system
    /// prompt alongside the base instructions. Empty by default
    /// so existing callers keep working unchanged.
    ///
    /// Stored behind `Arc<PromptContext>` so cloning an
    /// [`AgentRunConfig`] bumps a refcount instead of
    /// deep-cloning the manifest — every chat dispatch clones
    /// this config to thread it through the run factory.
    pub prompt_context: Arc<PromptContext>,
    /// Optional sync resolver that turns the [`Self::agent_name`]
    /// into an [`AgentDescriptor`](crate::agent::AgentDescriptor).
    ///
    /// When `Some`, the run factory calls `resolver(name)`
    /// **before** constructing the [`ReActAgent`](crate::agent::ReActAgent) and uses the
    /// returned descriptor as the agent's identity (overriding
    /// the [`Self::system_prompt`] field). When `None`, the
    /// factory falls back to the legacy "default ReActAgent"
    /// path that uses [`Self::system_prompt`] as the base
    /// instructions.
    ///
    /// Defined as a boxed sync callback (rather than carrying
    /// an `Arc<AppState>`) so this crate stays free of any
    /// synthia-server dependency.
    pub agent_resolver: Option<
        Arc<
            dyn Fn(String) -> Option<crate::agent::AgentDescriptor>
                + Send
                + Sync,
        >,
    >,
    /// Selected agent name. Ignored when
    pub agent_name: Option<String>,
    /// Per-run iteration cap applied to the agent this
    /// config builds. `None` falls back to
    /// [`crate::agent::DEFAULT_MAX_ITERATIONS`].
    /// The agent-side `[1, 4096]` clamp still applies.
    pub max_iterations: Option<usize>,
    /// Steering bundle (guards / hooks / hints / tracker /
    /// output transformer) installed on every agent the run
    /// factory builds. Shared `Arc` so per-dispatch clones are
    /// refcount bumps. Defaults to [`Steering::noop`] when the
    /// server layer does not supply a policy.
    pub steering: Arc<Steering>,
    /// Optional multi-agent registry. The factory passes it
    /// through so callers that want to compose
    /// multi-agent orchestration can build their own
    /// coordinator on top. When `None` the run factory
    /// treats panel descriptors like any other agent
    /// (no fan-out).
    pub agent_registry: Option<Arc<crate::agent::AgentRegistry>>,
    /// Typed-event sink the agent loop uses to publish
    /// structural boundary events (R6-A wiring):
    /// `request_header`, `iteration_start/end`,
    /// `step_start/end`, `usage`, `subagent_enter/exit`.
    /// `None` (the default) disables typed emission — the
    /// loop's in-memory AgentEvent stream is unaffected.
    pub typed_event_sink: Option<synthia_session::TypedEventSink>,
    /// R22: explicit context-manager override. The caller (the
    /// server layer, or any consumer composing capabilities)
    /// builds whatever manager it wants — e.g.
    /// `context_manager_for_compaction_with_emitters(provider,
    /// settings, None)`, a grounding manager wrapping that, or a
    /// custom chain — and hands it in here. `None` keeps the loop's
    /// default (truncating) manager.
    ///
    /// This replaces the R16 `compaction` field: one generic seam
    /// beats one capability-specific field, and it keeps this
    /// crate free of any dependency on the compaction crates.
    pub context_manager: Option<Arc<dyn synthia_context::ContextManager>>,
    /// R34: deployment-level tool-surface policy installed on the
    /// agent this config builds (see
    /// [`crate::agent::ReActAgent::with_tool_surface`]). `None`
    /// (the default) leaves the R33 projection untouched.
    ///
    /// The policy rides the run config — rather than living only on
    /// the registry — because `max_visible` caps one request's
    /// advertisement and is not a registry write; the group verdict
    /// has already been applied to the registry's exposures at boot.
    pub tool_surface: Option<synthia_tool::ToolSurfacePolicy>,
    /// R50: the reasoning loop this run should use. `None` keeps
    /// [`ReActAgent`](crate::agent::ReActAgent)'s default
    /// ([`ReActStrategy`](crate::agent::ReActStrategy)).
    ///
    /// This is the deployment-facing hop of the R49 seam: an operator
    /// names a strategy in config, the server resolves it once at boot
    /// with [`crate::agent::from_name`], and the run factory
    /// installs it here. A consumer that composes its own runtime sets
    /// it directly.
    pub strategy: Option<Arc<dyn ReasoningStrategy>>,
    /// R58: the agent's own allow/deny list. The factory installs it
    /// with [`ReActAgent::with_tool_restriction`](crate::agent::ReActAgent::with_tool_restriction),
    /// which keeps the denied tools out of the model's list *and*
    /// refuses a call to one. `None` (the default) means "everything
    /// the surface advertises".
    pub tool_restriction: Option<Arc<synthia_tool::ToolRestriction>>,
}

#[cfg(test)]
mod tests {
    use synthia_provider::traits_stub::ModelProviderStub;

    use super::*;

    fn make_minimal_config() -> AgentRunConfig {
        let provider = Arc::new(ModelProviderStub::text_only("hi"));
        let tool_registry = Arc::new(ToolRegistry::new());
        AgentRunConfig {
            provider,
            tool_registry,
            workspace_root: PathBuf::from("/tmp/test"),
            system_prompt: String::from("You are a test agent."),
            prompt_context: Arc::new(PromptContext::default()),
            agent_resolver: None,
            agent_name: None,
            max_iterations: None,
            steering: Arc::new(Steering::noop()),
            agent_registry: None,
            typed_event_sink: None,
            context_manager: None,
            tool_surface: None,
            strategy: None,
            tool_restriction: None,
        }
    }

    /// `AgentRunConfig` MUST support direct field construction.
    #[test]
    fn direct_construction() {
        let c = make_minimal_config();
        assert_eq!(c.system_prompt, "You are a test agent.");
        assert_eq!(c.workspace_root, PathBuf::from("/tmp/test"));
        assert!(c.agent_resolver.is_none());
        assert!(c.agent_name.is_none());
        assert!(c.agent_registry.is_none());
    }

    /// `AgentRunConfig` MUST support `Clone` (cheap — all
    /// fields are `Arc` or `Copy`).
    #[test]
    fn supports_clone() {
        let c = make_minimal_config();
        let cloned = c.clone();
        assert_eq!(cloned.system_prompt, c.system_prompt);
        assert_eq!(cloned.workspace_root, c.workspace_root);
        // Arc clone — same pointer.
        assert!(Arc::ptr_eq(
            &cloned.provider as &Arc<dyn ModelProvider>,
            &c.provider as &Arc<dyn ModelProvider>,
        ));
    }

    /// `workspace_root: PathBuf` MUST accept any path string.
    #[test]
    fn workspace_root_accepts_any_path() {
        let mut c = make_minimal_config();
        c.workspace_root = PathBuf::from("/absolute/path");
        assert_eq!(c.workspace_root, PathBuf::from("/absolute/path"));
        c.workspace_root = PathBuf::from("relative/path");
        assert_eq!(c.workspace_root, PathBuf::from("relative/path"));
    }

    /// `system_prompt: String` MUST accept empty string.
    #[test]
    fn system_prompt_accepts_empty_string() {
        let mut c = make_minimal_config();
        c.system_prompt = String::new();
        assert_eq!(c.system_prompt, "");
    }

    /// `system_prompt` MUST preserve multi-line content.
    #[test]
    fn system_prompt_preserves_multiline() {
        let mut c = make_minimal_config();
        c.system_prompt = "line1\nline2\nline3".to_string();
        assert_eq!(c.system_prompt.lines().count(), 3);
    }

    /// `agent_resolver: Option<Arc<dyn Fn>>` MUST be
    /// settable and replaceable.
    #[test]
    fn agent_resolver_settable_and_replaceable() {
        let mut c = make_minimal_config();
        // Initial: None.
        assert!(c.agent_resolver.is_none());

        // Set to Some(resolver).
        let resolver: Arc<
            dyn Fn(String) -> Option<crate::agent::AgentDescriptor>
                + Send
                + Sync,
        > = Arc::new(|_name| None);
        c.agent_resolver = Some(resolver);
        assert!(c.agent_resolver.is_some());

        // Replace back to None.
        c.agent_resolver = None;
        assert!(c.agent_resolver.is_none());
    }

    /// `agent_name: Option<String>` MUST be settable.
    #[test]
    fn agent_name_settable() {
        let mut c = make_minimal_config();
        c.agent_name = Some("my-agent".to_string());
        assert_eq!(c.agent_name, Some("my-agent".to_string()));
        c.agent_name = None;
        assert!(c.agent_name.is_none());
    }

    /// `prompt_context: PromptContext` MUST default to all
    /// empty lists.
    #[test]
    fn prompt_context_defaults_to_empty_lists() {
        let c = make_minimal_config();
        assert!(c.prompt_context.skills.is_empty());
        assert!(c.prompt_context.agents.is_empty());
    }

    /// `prompt_context` MUST be settable to non-empty values
    /// for skills / peer agents via the public builder.
    /// Tool schemas are deliberately absent from
    /// `PromptContext` — they ride the completion request's
    /// `tools` channel instead.
    #[test]
    fn prompt_context_settable_via_builder() {
        let mut c = make_minimal_config();
        let peer = crate::agent::AgentDescriptor {
            name: "planner".into(),
            description: "Plans the work.".into(),
            kind: "planner".into(),
            version: "1.0.0".into(),
            instructions: "".into(),
            capabilities: Vec::new(),
            tools: Vec::new(),
            model_hint: None,
            handoffs: Vec::new(),
            handoff_hint: Some("complex tasks".into()),
            output_schema: None,
            owner: None,
            domain: None,
            persona: None,
            display_name: None,
            max_iterations: None,
        };
        c.prompt_context = Arc::new(
            crate::prompt::PromptContext::default()
                .with_skill("summarize", "Summarize text.")
                .with_agent(&peer),
        );
        assert_eq!(c.prompt_context.skills.len(), 1);
        assert_eq!(c.prompt_context.agents.len(), 1);
    }
}
