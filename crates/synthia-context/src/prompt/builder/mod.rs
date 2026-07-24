//! Prompt builder submodules: state management, builder, and config.
//!
//! - [`state`]: `PromptState`, `ResolvedPrompt`, `CacheStats`, `SystemPromptPriority`
//! - [`builder`]: `PromptBuilder` — the actual section orchestrator
//! - [`config`]: `EffectivePromptConfig` — high-level prompt overrides
//!
//! Test fixtures shared across the test modules below
//! (`TEST_AGENT_NAME`, `make_test_context`, `MockSection`) are
//! defined here so each submodule's `#[cfg(test)] mod tests` can
//! import them via `use super::*`.

// `mod builder` shares its name with the containing `builder/` directory.
// This is intentional — `builder.rs` is the orchestrator (PromptBuilder) and
// lives as a peer to `state.rs` and `config.rs` under the same-named directory.
#[allow(clippy::module_inception)]
mod builder;
mod config;
mod state;

pub use builder::PromptBuilder;
pub use config::EffectivePromptConfig;
pub use state::{
    CacheStats,
    PromptState,
    ResolvedPrompt,
    SystemPromptPriority,
};

#[cfg(test)]
pub(super) mod test_support {
    use std::sync::LazyLock;

    use crate::prompt::{
        AgentName,
        PromptContext,
        section_trait::SectionCaching,
        sections::PromptSection,
    };

    pub static TEST_AGENT_NAME: LazyLock<AgentName> =
        LazyLock::new(|| AgentName::Custom("TestAgent".to_string()));

    pub fn make_test_context() -> PromptContext<'static> {
        PromptContext {
            agent_name: &TEST_AGENT_NAME,
            agent_description: "A test agent",
            workspace_dir: std::path::Path::new("/tmp/test"),
            skill_instructions: String::new(),
            is_subagent: false,
            session_id: Some("test-session-123"),
            mcp_servers: &[],
            additional_dirs: &[],
            output_style: None,
            language_preference: None,
            is_proactive_mode: false,
            model_name: Some("claude-sonnet"),
            knowledge_cutoff: Some("2024-01"),
            team_info: None,
        }
    }

    pub struct MockSection {
        pub name: String,
        pub caching: SectionCaching,
        pub content: String,
    }

    impl MockSection {
        pub fn new(name: &str, caching: SectionCaching, content: &str) -> Self {
            Self {
                name: name.to_string(),
                caching,
                content: content.to_string(),
            }
        }
    }

    impl PromptSection for MockSection {
        fn name(&self) -> &str {
            &self.name
        }

        fn caching(&self) -> SectionCaching {
            self.caching
        }

        fn build(&self, _ctx: &PromptContext<'_>) -> anyhow::Result<String> {
            Ok(self.content.clone())
        }
    }
}
