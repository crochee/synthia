use super::{PromptSection, prepend_bullets};
use crate::prompt::{Result, SectionCaching};

const TOOL_GUIDANCE_ITEMS: &[&str] = &[
    "Use dedicated tools over Bash: Read>cat, Edit>sed, Write>heredoc, Grep>grep, Glob>find",
    "Safe to parallelize: Read, Grep, Glob, WebSearch, WebFetch",
    "Must run serially: Edit, Write, Delete, Bash, MoveFile",
];

#[derive(Debug, Clone, Copy, Default)]
pub struct ToolUsageSection;

impl ToolUsageSection {
    pub fn new() -> Self {
        Self
    }
}

impl PromptSection for ToolUsageSection {
    fn name(&self) -> &str {
        "tool_usage"
    }

    fn caching(&self) -> SectionCaching {
        SectionCaching::Cached
    }

    fn build(&self, _ctx: &crate::prompt::PromptContext<'_>) -> Result<String> {
        Ok(format!(
            "# Tool Guidance\n{}",
            prepend_bullets(TOOL_GUIDANCE_ITEMS)
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::LazyLock};

    use super::*;
    use crate::{
        config::AgentName,
        prompt::{McpServerInfo, PromptContext, SectionCaching},
    };

    static TEST_AGENT_NAME: LazyLock<AgentName> =
        LazyLock::new(|| AgentName::Custom("TestAgent".to_string()));

    fn make_basic_ctx<'a>(
        workspace_dir: &'a PathBuf,
        additional_dirs: &'a [PathBuf],
        mcp_servers: &'a [McpServerInfo],
    ) -> PromptContext<'a> {
        PromptContext {
            agent_name: &TEST_AGENT_NAME,
            agent_description: "A test agent",
            workspace_dir,
            skill_instructions: String::new(),
            is_subagent: false,
            session_id: Some("test-session"),
            mcp_servers,
            additional_dirs,
            output_style: None,
            language_preference: Some("en-US"),
            is_proactive_mode: false,
            model_name: Some("claude-sonnet-4-6"),
            knowledge_cutoff: Some("2026-03-01"),
            team_info: None,
        }
    }

    #[test]
    fn test_tool_usage_section_name() {
        let section = ToolUsageSection::new();
        assert_eq!(section.name(), "tool_usage");
    }

    #[test]
    fn test_tool_usage_section_caching() {
        let section = ToolUsageSection::new();
        assert_eq!(section.caching(), SectionCaching::Cached);
    }

    #[test]
    fn test_tool_usage_section_build() {
        let section = ToolUsageSection::new();
        let workspace_dir = PathBuf::from("/tmp/test");
        let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
        let content = section.build(&ctx).unwrap();
        assert!(content.contains("# Tool Guidance"));
        assert!(content.contains("Use dedicated tools over Bash"));
        assert!(content.contains("Safe to parallelize"));
    }

    #[test]
    fn test_tool_usage_section_build_contains_guidance_items() {
        let section = ToolUsageSection::new();
        let workspace_dir = PathBuf::from("/tmp/test");
        let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
        let content = section.build(&ctx).unwrap();
        // Check all TOOL_GUIDANCE_ITEMS are present
        assert!(content.contains("Use dedicated tools over Bash: Read>cat, Edit>sed, Write>heredoc, Grep>grep, Glob>find"));
        assert!(content.contains(
            "Safe to parallelize: Read, Grep, Glob, WebSearch, WebFetch"
        ));
        assert!(content.contains(
            "Must run serially: Edit, Write, Delete, Bash, MoveFile"
        ));
    }

    #[test]
    fn test_tool_usage_section_new() {
        let section = ToolUsageSection::new();
        assert_eq!(section.name(), "tool_usage");
    }

    #[test]
    fn test_tool_usage_section_clone() {
        let section = ToolUsageSection::new();
        let cloned = section;
        assert_eq!(section.name(), cloned.name());
        assert_eq!(section.caching(), cloned.caching());
    }

    #[test]
    fn test_tool_usage_section_default() {
        let section = ToolUsageSection;
        assert_eq!(section.name(), "tool_usage");
    }
}
