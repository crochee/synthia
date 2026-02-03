use super::{PromptSection, prepend_bullets};
use crate::prompt::{Result, SectionCaching};

const SYSTEM_ITEMS: &[&str] = &[
    "All text outside tool use is displayed to the user. Use Markdown, avoid emojis.",
    "Tool permission mode controls access. If denied, adjust approach.",
    "Tool results and user messages may include <system-reminder> tags. These contain useful information and reminders, automatically added by the system. They bear no direct relation to the specific tool results or messages in which they appear.",
    "Hook feedback is user input. Adapt or ask about config.",
    "Always use absolute paths (not relative) for file operations.",
    "Share relevant file paths in responses (absolute, not relative).",
];

const COMMUNICATION_ITEMS: &[&str] = &[
    "Short and direct. Lead with answer/action.",
    "Use `file:line` for code references, `owner/repo#123` for GitHub.",
    "Keep text between tool calls ≤25 words. Final responses ≤100 words unless task requires more.",
];

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemSection;

impl SystemSection {
    pub fn new() -> Self {
        Self
    }
}

impl PromptSection for SystemSection {
    fn name(&self) -> &str {
        "system"
    }

    fn caching(&self) -> SectionCaching {
        SectionCaching::Cached
    }

    fn build(&self, _ctx: &crate::prompt::PromptContext<'_>) -> Result<String> {
        Ok(format!(
            "# System\n\n{}\n\n# Communication\n\n{}",
            prepend_bullets(SYSTEM_ITEMS),
            prepend_bullets(COMMUNICATION_ITEMS),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::prompt::{McpServerInfo, PromptContext, SectionCaching};

    fn make_basic_ctx<'a>(
        workspace_dir: &'a PathBuf,
        additional_dirs: &'a [PathBuf],
        mcp_servers: &'a [McpServerInfo],
    ) -> PromptContext<'a> {
        PromptContext {
            agent_name: "TestAgent",
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
        }
    }

    #[test]
    fn test_system_section_name() {
        let section = SystemSection::new();
        assert_eq!(section.name(), "system");
    }

    #[test]
    fn test_system_section_caching() {
        let section = SystemSection::new();
        assert_eq!(section.caching(), SectionCaching::Cached);
    }

    #[test]
    fn test_system_section_build() {
        let section = SystemSection::new();
        let workspace_dir = PathBuf::from("/tmp/test");
        let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
        let content = section.build(&ctx).unwrap();
        assert!(content.contains("# System"));
        assert!(content.contains("# Communication"));
        assert!(content.contains("All text outside tool use"));
        assert!(content.contains("Short and direct"));
    }

    #[test]
    fn test_system_section_build_contains_system_items() {
        let section = SystemSection::new();
        let workspace_dir = PathBuf::from("/tmp/test");
        let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
        let content = section.build(&ctx).unwrap();
        // Check all SYSTEM_ITEMS are present
        assert!(
            content
                .contains("All text outside tool use is displayed to the user")
        );
        assert!(content.contains("Tool permission mode controls access"));
        assert!(content.contains("Hook feedback is user input"));
        assert!(content.contains("Always use absolute paths"));
        assert!(content.contains("Share relevant file paths in responses"));
    }

    #[test]
    fn test_system_section_build_contains_communication_items() {
        let section = SystemSection::new();
        let workspace_dir = PathBuf::from("/tmp/test");
        let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
        let content = section.build(&ctx).unwrap();
        // Check all COMMUNICATION_ITEMS are present
        assert!(content.contains("Short and direct. Lead with answer/action"));
        assert!(content.contains("Use `file:line` for code references"));
        assert!(content.contains("Keep text between tool calls"));
    }

    #[test]
    fn test_system_section_new() {
        let section = SystemSection::new();
        assert_eq!(section.name(), "system");
    }

    #[test]
    fn test_system_section_clone() {
        let section = SystemSection::new();
        let cloned = section;
        assert_eq!(section.name(), cloned.name());
        assert_eq!(section.caching(), cloned.caching());
    }

    #[test]
    fn test_system_section_default() {
        let section = SystemSection;
        assert_eq!(section.name(), "system");
    }
}
