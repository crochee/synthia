use std::{path::PathBuf, sync::Arc};

use synthia_provider::traits::ModelProvider;
use synthia_tool::registry::ToolRegistry;

use crate::prompt::PromptContext;

/// Per-session configuration consumed by the
/// [`crate::agent::Agent::run`] factory inside
/// `SessionController`.
///
/// Carries the shared dependencies (provider, tool registry)
/// plus the per-session manifest and the optional sync resolver
/// used to bind a named agent to its [`AgentDescriptor`].
///
/// [`AgentDescriptor`]: crate::agent::descriptor::AgentDescriptor
#[derive(Clone)]
pub struct AgentRunConfig {
    pub provider: Arc<dyn ModelProvider>,
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
    /// the [`crate::prompt`] assembler renders into the system
    /// prompt alongside the base instructions. Empty by default
    /// so existing callers keep working unchanged.
    ///
    /// Stored behind `Arc<PromptContext>` so cloning an
    /// [`AgentRunConfig`] bumps a refcount instead of
    /// deep-cloning the manifest — every chat dispatch clones
    /// this config to thread it through the run factory.
    pub prompt_context: Arc<PromptContext>,
    /// Optional sync resolver that turns the [`Self::agent_name`]
    /// into an [`AgentDescriptor`].
    ///
    /// When `Some`, the run factory calls `resolver(name)`
    /// **before** constructing the [`ReActAgent`] and uses the
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
            dyn Fn(String) -> Option<crate::agent::descriptor::AgentDescriptor>
                + Send
                + Sync,
        >,
    >,
    /// Selected agent name. Ignored when
    /// [`Self::agent_resolver`] is `None`.
    pub agent_name: Option<String>,
    /// Optional multi-agent registry. The factory passes it
    /// through so callers that want to compose
    /// multi-agent orchestration can build their own
    /// coordinator on top. When `None` the run factory
    /// treats panel descriptors like any other agent
    /// (no fan-out).
    pub agent_registry: Option<Arc<crate::agent::registry::AgentRegistry>>,
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
            agent_registry: None,
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
            dyn Fn(String) -> Option<crate::agent::descriptor::AgentDescriptor>
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
        let peer = crate::agent::descriptor::AgentDescriptor {
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
