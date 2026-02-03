//! System prompt builder with modular sections.
//!
//! Key concepts:
//! - **SectionCaching**: Caching levels (Cached, SessionCached, Volatile, Uncached)
//! - **PromptSection**: Unified trait for prompt sections
//! - **PromptState**: State management for caching
//! - **PromptBuilder**: Builds prompts from sections with caching
//! - **PromptLatches**: Manages mid-session state that should persist
//! - **SYSTEM_PROMPT_DYNAMIC_BOUNDARY**: Separates static/dynamic content
//!
//! ## Static/Dynamic Separation
//!
//! The system prompt is divided at `SYSTEM_PROMPT_DYNAMIC_BOUNDARY`:
//! - **Before boundary**: Static content (Cached) - can be cached globally across sessions
//! - **After boundary**: Dynamic content (SessionCached/Volatile) - session-specific
//!
//! This separation enables prompt caching at the API level, reducing costs.

mod builder;
mod cache;
mod compaction;
mod constants;
mod latches;
mod section_trait;

pub mod sections;

use std::path::PathBuf;

pub use builder::{
    CacheStats,
    EffectivePromptConfig,
    PromptBuilder,
    PromptState,
    ResolvedPrompt,
    SystemPromptPriority,
};
pub use cache::{
    CacheBreakDetector,
    CacheBreakReport,
    PromptStateSnapshot,
    TrackedState,
    create_prompt_snapshot,
};
pub use compaction::{
    AUTO_COMPACT_CONTINUATION_TEXT,
    COMPACTION_SYSTEM_PROMPT,
    COMPACTION_USER_PROMPT,
    CONVERSATION_CONTINUATION_TEXT,
    CompactionType,
    MANUAL_COMPACT_CONTINUATION_TEXT,
    TOOL_LOOP_CONTINUATION_TEXT,
    format_compact_summary,
    render_compaction_prompt,
    render_compaction_prompt_with_type,
};
pub use constants::{
    OutputStyleConfig,
    SYSTEM_PROMPT_DYNAMIC_BOUNDARY,
    TITLE_SYSTEM_PROMPT,
};
pub use latches::PromptLatches;
pub use section_trait::SectionCaching;
pub use sections::{PromptSection, *};

use crate::Result;

#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub instructions: Option<String>,
}

#[derive(Clone)]
pub struct PromptContext<'a> {
    pub agent_name: &'a str,
    pub agent_description: &'a str,
    pub workspace_dir: &'a std::path::Path,
    pub skill_instructions: String,
    pub is_subagent: bool,
    pub session_id: Option<&'a str>,
    pub mcp_servers: &'a [McpServerInfo],
    pub additional_dirs: &'a [PathBuf],
    pub output_style: Option<&'a OutputStyleConfig>,
    pub language_preference: Option<&'a str>,
    pub is_proactive_mode: bool,
    pub model_name: Option<&'a str>,
    pub knowledge_cutoff: Option<&'a str>,
}

impl std::fmt::Debug for PromptContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PromptContext")
            .field("agent_name", &self.agent_name)
            .field("agent_description", &self.agent_description)
            .field("workspace_dir", &self.workspace_dir)
            .field("skill_instructions", &self.skill_instructions.len())
            .field("is_subagent", &self.is_subagent)
            .field("session_id", &self.session_id)
            .field("mcp_servers", &self.mcp_servers.len())
            .field("additional_dirs", &self.additional_dirs.len())
            .field("model_name", &self.model_name)
            .field("knowledge_cutoff", &self.knowledge_cutoff)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolved_prompt_full() {
        let resolved = ResolvedPrompt {
            static_content: "static".to_string(),
            dynamic_content: "dynamic".to_string(),
            sections_used: vec!["test".to_string()],
            prefix_hash: "abc".to_string(),
            static_hash: "def".to_string(),
        };

        let full = resolved.full_prompt();
        assert!(full.contains("static"));
        assert!(full.contains(SYSTEM_PROMPT_DYNAMIC_BOUNDARY));
        assert!(full.contains("dynamic"));
    }

    #[test]
    fn test_resolved_prompt_no_dynamic() {
        let resolved = ResolvedPrompt {
            static_content: "static only".to_string(),
            dynamic_content: String::new(),
            sections_used: vec![],
            prefix_hash: "abc".to_string(),
            static_hash: "def".to_string(),
        };

        assert_eq!(resolved.full_prompt(), "static only");
    }

    #[test]
    fn test_prompt_context_debug() {
        let ctx = PromptContext {
            agent_name: "test",
            agent_description: "test agent",
            workspace_dir: std::path::Path::new("/tmp"),
            skill_instructions: String::new(),
            is_subagent: false,
            session_id: None,
            mcp_servers: &[],
            additional_dirs: &[],
            output_style: None,
            language_preference: None,
            is_proactive_mode: false,
            model_name: None,
            knowledge_cutoff: None,
        };

        let debug = format!("{ctx:?}");
        assert!(debug.contains("test"));
        assert!(debug.contains("/tmp"));
    }

    #[test]
    fn test_effective_prompt_config_builder() {
        let config = EffectivePromptConfig::new()
            .with_override("override".to_string())
            .with_coordinator("coordinator".to_string())
            .with_agent("agent".to_string())
            .with_custom("custom".to_string())
            .with_append("append".to_string())
            .with_coordinator_mode(true);

        assert_eq!(config.override_prompt, Some("override".to_string()));
        assert_eq!(config.coordinator_prompt, Some("coordinator".to_string()));
        assert_eq!(config.agent_prompt, Some("agent".to_string()));
        assert_eq!(config.custom_prompt, Some("custom".to_string()));
        assert_eq!(config.append_prompt, Some("append".to_string()));
        assert!(config.use_coordinator_mode);
    }

    #[test]
    fn test_effective_prompt_config_default() {
        let config = EffectivePromptConfig::new();
        assert!(config.override_prompt.is_none());
        assert!(config.coordinator_prompt.is_none());
        assert!(config.agent_prompt.is_none());
        assert!(config.custom_prompt.is_none());
        assert!(config.append_prompt.is_none());
        assert!(!config.use_coordinator_mode);
    }

    #[test]
    fn test_prompt_state_new() {
        let state = PromptState::new();
        assert_eq!(state.stats().global_entries, 0);
        assert_eq!(state.stats().session_entries, 0);
    }

    #[test]
    fn test_prompt_state_insert_and_get() {
        let mut state = PromptState::new();

        // Insert with session caching
        state.insert(
            "key1".to_string(),
            "value1".to_string(),
            SectionCaching::SessionCached,
        );
        assert_eq!(
            state.get("key1", SectionCaching::SessionCached),
            Some("value1".to_string())
        );
        assert_eq!(state.get("key1", SectionCaching::Cached), None);

        // Insert with global caching
        state.insert(
            "key2".to_string(),
            "value2".to_string(),
            SectionCaching::Cached,
        );
        assert_eq!(
            state.get("key2", SectionCaching::Cached),
            Some("value2".to_string())
        );
        assert_eq!(state.get("key2", SectionCaching::SessionCached), None);

        // Uncached returns none
        assert_eq!(state.get("key1", SectionCaching::Uncached), None);
    }

    #[test]
    fn test_prompt_state_clear_session() {
        let mut state = PromptState::new();

        state.insert(
            "key1".to_string(),
            "value1".to_string(),
            SectionCaching::SessionCached,
        );
        state.insert(
            "key2".to_string(),
            "value2".to_string(),
            SectionCaching::Cached,
        );

        state.clear_session();

        assert_eq!(state.get("key1", SectionCaching::SessionCached), None);
        assert_eq!(
            state.get("key2", SectionCaching::Cached),
            Some("value2".to_string())
        );
    }

    #[test]
    fn test_prompt_state_clear_all() {
        let mut state = PromptState::new();

        state.insert(
            "key1".to_string(),
            "value1".to_string(),
            SectionCaching::SessionCached,
        );
        state.insert(
            "key2".to_string(),
            "value2".to_string(),
            SectionCaching::Cached,
        );

        state.clear_all();

        assert_eq!(state.stats().global_entries, 0);
        assert_eq!(state.stats().session_entries, 0);
    }

    #[test]
    fn test_prompt_state_invalidate() {
        let mut state = PromptState::new();

        state.insert(
            "key1".to_string(),
            "value1".to_string(),
            SectionCaching::SessionCached,
        );
        state.insert(
            "key2".to_string(),
            "value2".to_string(),
            SectionCaching::SessionCached,
        );

        state.invalidate("key1");

        assert_eq!(state.get("key1", SectionCaching::SessionCached), None);
        assert_eq!(
            state.get("key2", SectionCaching::SessionCached),
            Some("value2".to_string())
        );
    }

    #[test]
    fn test_system_prompt_priority_order() {
        use SystemPromptPriority::*;

        let priorities = vec![Override, Coordinator, Agent, Custom, Default];
        for p in &priorities {
            let _ = format!("{p:?}");
        }
    }

    #[test]
    fn test_cache_stats_debug() {
        let stats = CacheStats {
            global_entries: 5,
            session_entries: 3,
        };
        let debug = format!("{stats:?}");
        assert!(debug.contains("5"));
        assert!(debug.contains("3"));
    }

    #[test]
    fn test_mcp_server_info() {
        let info = McpServerInfo {
            name: "test-server".to_string(),
            instructions: Some("Use this server".to_string()),
        };
        assert_eq!(info.name, "test-server");
        assert!(info.instructions.is_some());
    }

    #[test]
    fn test_mcp_server_info_no_instructions() {
        let info = McpServerInfo {
            name: "test-server".to_string(),
            instructions: None,
        };
        assert!(info.instructions.is_none());
    }
}
