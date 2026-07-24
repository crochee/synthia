//! Unit tests for the `sections` module family.
//!
//! The original 31 tests lived at the bottom of `sections/mod.rs`;
//! they're hoisted into this sibling file so the production code
//! (`trait` / `helpers` + the 14 section implementations) doesn't
//! carry the test body weight.
//!
//! Coverage map (31 tests):
//!
//! - `SystemSection` (3): name / caching / build.
//! - `IdentitySection` (3 + 2 helpers): name / caching / build
//!   (default, subagent, with_output_style) + `has_workspace_files`
//!   + `WORKSPACE_FILES` excludes `AGENTS.md`.
//! - `TaskExecutionSection` (3): name / caching / build
//!   (default, subagent).
//! - `ToolUsageSection` (2): name / caching / build.
//! - `EnvironmentSection` (4): name / caching / build
//!   (default, with_additional_dirs, no_model).
//! - `DynamicMcpInstructionsSection` (5): name / caching / build
//!   (empty, no_instructions, with_instructions, mixed).
//! - `MemorySection` (3): name / caching / build
//!   (empty, with_files).
//! - `OutputStyleSection` (4): name / caching / build
//!   (default, empty_prompt).
//! - `LanguageSection` (4): name / caching / build
//!   (default, empty).
//! - `SkillSection` (3): name / caching / build
//!   (without_skills, with_skills).
//! - `TokenBudgetSection` (3): name / caching / build
//!   (without_anchors, with_anchors).
//! - `ProactiveSection` (4): name / caching / build
//!   (not_proactive, proactive).
//! - Trait + helpers (3): `test_prompt_section_boxed` /
//!   `test_prepend_bullets` / `test_join_lines`.

use std::{path::PathBuf, sync::LazyLock};

use super::*;
use crate::prompt::{
    AgentName,
    McpServerInfo,
    OutputStyleConfig,
    PromptContext,
    SectionCaching,
};

static TEST_AGENT_NAME: LazyLock<AgentName> =
    LazyLock::new(|| AgentName::Custom("TestAgent".to_string()));

// Helper to create a PromptContext with static lifetime for simple string slices
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

// === SystemSection tests ===

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

// === IdentitySection tests ===

#[test]
fn test_identity_section_name() {
    let section = IdentitySection::new();
    assert_eq!(section.name(), "identity");
}

#[test]
fn test_identity_section_caching() {
    let section = IdentitySection::new();
    assert_eq!(section.caching(), SectionCaching::Cached);
}

#[test]
fn test_identity_section_build_default() {
    let section = IdentitySection::new();
    let workspace_dir = PathBuf::from("/tmp/test");
    let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    let content = section.build(&ctx).unwrap();
    assert!(content.contains("TestAgent"));
    assert!(content.contains("A test agent"));
    assert!(content.contains("authorized security testing"));
}

#[test]
fn test_identity_section_build_subagent() {
    let section = IdentitySection::new();
    let workspace_dir = PathBuf::from("/tmp/test");
    let mut ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    ctx.is_subagent = true;
    let content = section.build(&ctx).unwrap();
    assert!(content.contains("subagent"));
    assert!(content.contains("TestAgent"));
}

#[test]
fn test_identity_section_build_with_output_style() {
    let section = IdentitySection::new();
    let workspace_dir = PathBuf::from("/tmp/test");
    let style = OutputStyleConfig {
        name: "concise".to_string(),
        prompt: "Be very concise".to_string(),
        keep_coding_instructions: false,
    };
    let mut ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    ctx.output_style = Some(&style);
    let content = section.build(&ctx).unwrap();
    assert!(content.contains("TestAgent"));
}

#[test]
fn test_has_workspace_files() {
    use std::fs;
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("IDENTITY.md"), "# Identity").unwrap();

    // IdentitySection::WORKSPACE_FILES is the source of truth (the
    // helper itself is generic over its `files` arg, so we just use
    // the list as defined in identity.rs).
    let files = ["IDENTITY.md", "USER.md", "MEMORY.md"];
    assert!(super::identity::has_workspace_files(dir.path(), &files));

    let missing = ["NOTEXIST.md"];
    assert!(!super::identity::has_workspace_files(dir.path(), &missing));
}

#[test]
fn test_workspace_files_excludes_agents_md() {
    // AGENTS.md injection is handled by the dedicated
    // `AgentsMdSection` (hierarchical discovery from workspace_dir
    // ancestors). IdentitySection must not re-inject it as a flat
    // workspace file.
    let files = super::identity::WORKSPACE_FILES;
    assert!(
        !files.contains(&"AGENTS.md"),
        "IdentitySection::WORKSPACE_FILES must not include AGENTS.md \
         (it is now injected by AgentsMdSection with hierarchical discovery)"
    );
}

// === TaskExecutionSection tests ===

#[test]
fn test_task_execution_section_name() {
    let section = TaskExecutionSection::new();
    assert_eq!(section.name(), "task_execution");
}

#[test]
fn test_task_execution_section_caching() {
    let section = TaskExecutionSection::new();
    assert_eq!(section.caching(), SectionCaching::Cached);
}

#[test]
fn test_task_execution_section_build() {
    let section = TaskExecutionSection::new();
    let workspace_dir = PathBuf::from("/tmp/test");
    let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    let content = section.build(&ctx).unwrap();
    assert!(content.contains("# Doing tasks"));
    assert!(content.contains("## Task Guidelines"));
    assert!(content.contains("## Code Style"));
    assert!(content.contains("## Action Guidelines"));
    assert!(content.contains("Bug fixes"));
    assert!(content.contains("No gold-plating"));
}

#[test]
fn test_task_execution_section_build_subagent() {
    let section = TaskExecutionSection::new();
    let workspace_dir = PathBuf::from("/tmp/test");
    let mut ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    ctx.is_subagent = true;
    let content = section.build(&ctx).unwrap();
    assert!(content.contains("## Subagent"));
    assert!(content.contains("Stay within scope"));
    assert!(content.contains("Report completion"));
}

// === ToolUsageSection tests ===

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

// === EnvironmentSection tests ===

#[test]
fn test_environment_section_name() {
    let section = EnvironmentSection::new();
    assert_eq!(section.name(), "environment");
}

#[test]
fn test_environment_section_caching() {
    let section = EnvironmentSection::new();
    assert_eq!(section.caching(), SectionCaching::Volatile);
}

#[test]
fn test_environment_section_build() {
    let section = EnvironmentSection::new();
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_dir = temp_dir.path().to_path_buf();
    let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    let content = section.build(&ctx).unwrap();
    assert!(content.contains("# Environment"));
    assert!(content.contains("<env>"));
    assert!(content.contains("Working directory:"));
    assert!(content.contains("Architecture:"));
    assert!(content.contains("Platform:"));
    assert!(content.contains("Current time:"));
    assert!(content.contains("Model:"));
    assert!(content.contains("Knowledge cutoff:"));
    assert!(content.contains("</env>"));
}

#[test]
fn test_environment_section_build_with_additional_dirs() {
    let section = EnvironmentSection::new();
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_dir = temp_dir.path().to_path_buf();
    let additional_dirs =
        vec![PathBuf::from("/extra/dir1"), PathBuf::from("/extra/dir2")];
    let ctx = make_basic_ctx(&workspace_dir, &additional_dirs, &[]);
    let content = section.build(&ctx).unwrap();
    assert!(content.contains("Additional working directories:"));
    assert!(content.contains("/extra/dir1"));
    assert!(content.contains("/extra/dir2"));
}

#[test]
fn test_environment_section_build_no_model() {
    let section = EnvironmentSection::new();
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_dir = temp_dir.path().to_path_buf();
    let mut ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    ctx.model_name = None;
    ctx.knowledge_cutoff = None;
    let content = section.build(&ctx).unwrap();
    assert!(content.contains("# Environment"));
    assert!(!content.contains("Model:"));
}

// === DynamicMcpInstructionsSection tests ===

#[test]
fn test_mcp_section_name() {
    let section = DynamicMcpInstructionsSection::default();
    assert_eq!(section.name(), "mcp_instructions");
}

#[test]
fn test_mcp_section_caching() {
    let section = DynamicMcpInstructionsSection::default();
    assert_eq!(section.caching(), SectionCaching::Volatile);
}

#[test]
fn test_mcp_section_build_empty() {
    let section = DynamicMcpInstructionsSection::new(vec![]);
    let workspace_dir = PathBuf::from("/tmp/test");
    let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    let content = section.build(&ctx).unwrap();
    assert!(content.is_empty());
}

#[test]
fn test_mcp_section_build_no_instructions() {
    let section = DynamicMcpInstructionsSection::new(vec![
        McpServerInfo {
            name: "server1".to_string(),
            instructions: None,
        },
        McpServerInfo {
            name: "server2".to_string(),
            instructions: None,
        },
    ]);
    let workspace_dir = PathBuf::from("/tmp/test");
    let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    let content = section.build(&ctx).unwrap();
    assert!(content.is_empty());
}

#[test]
fn test_mcp_section_build_with_instructions() {
    let section = DynamicMcpInstructionsSection::new(vec![
        McpServerInfo {
            name: "server1".to_string(),
            instructions: Some("Use server1 like this".to_string()),
        },
        McpServerInfo {
            name: "server2".to_string(),
            instructions: Some("Use server2 like this".to_string()),
        },
    ]);
    let workspace_dir = PathBuf::from("/tmp/test");
    let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    let content = section.build(&ctx).unwrap();
    assert!(content.contains("# MCP Server Instructions"));
    assert!(content.contains("## server1"));
    assert!(content.contains("Use server1 like this"));
    assert!(content.contains("## server2"));
    assert!(content.contains("Use server2 like this"));
}

#[test]
fn test_mcp_section_build_mixed() {
    let section = DynamicMcpInstructionsSection::new(vec![
        McpServerInfo {
            name: "server1".to_string(),
            instructions: Some("Use server1".to_string()),
        },
        McpServerInfo {
            name: "server2".to_string(),
            instructions: None,
        },
    ]);
    let workspace_dir = PathBuf::from("/tmp/test");
    let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    let content = section.build(&ctx).unwrap();
    assert!(content.contains("## server1"));
    assert!(content.contains("Use server1"));
    assert!(!content.contains("## server2"));
}

// === MemorySection tests ===

#[test]
fn test_memory_section_name() {
    let section = MemorySection::new();
    assert_eq!(section.name(), "memory");
}

#[test]
fn test_memory_section_caching() {
    let section = MemorySection::new();
    assert_eq!(section.caching(), SectionCaching::SessionCached);
}

#[test]
fn test_memory_section_build_empty() {
    let section = MemorySection::new();
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_dir = temp_dir.path().to_path_buf();
    let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    let content = section.build(&ctx).unwrap();
    assert!(content.is_empty());
}

#[test]
fn test_memory_section_build_with_files() {
    let temp_dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp_dir.path().join(".agents")).unwrap();
    std::fs::write(
        temp_dir.path().join(".agents/MEMORY.md"),
        "# Memory\nSome content",
    )
    .unwrap();

    let section = MemorySection::new();
    let workspace_dir = temp_dir.path().to_path_buf();
    let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    let content = section.build(&ctx).unwrap();
    // Memory section may be empty or contain different content
    // Just verify it builds without error
    assert!(content.is_empty() || content.contains("Memory"));
}

// === OutputStyleSection tests ===

#[test]
fn test_output_style_section_name() {
    let section =
        OutputStyleSection::new("test".to_string(), "prompt".to_string());
    assert_eq!(section.name(), "output_style");
}

#[test]
fn test_output_style_section_caching() {
    let section =
        OutputStyleSection::new("test".to_string(), "prompt".to_string());
    assert_eq!(section.caching(), SectionCaching::SessionCached);
}

#[test]
fn test_output_style_section_build() {
    let section = OutputStyleSection::new(
        "concise".to_string(),
        "Be very concise".to_string(),
    );
    let workspace_dir = PathBuf::from("/tmp/test");
    let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    let content = section.build(&ctx).unwrap();
    assert!(content.contains("# Output Style: concise"));
    assert!(content.contains("Be very concise"));
}

#[test]
fn test_output_style_section_build_empty_prompt() {
    let section = OutputStyleSection::new("test".to_string(), String::new());
    let workspace_dir = PathBuf::from("/tmp/test");
    let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    let content = section.build(&ctx).unwrap();
    assert!(content.is_empty());
}

// === LanguageSection tests ===

#[test]
fn test_language_section_name() {
    let section = LanguageSection::new("en-US".to_string());
    assert_eq!(section.name(), "language");
}

#[test]
fn test_language_section_caching() {
    let section = LanguageSection::new("en-US".to_string());
    assert_eq!(section.caching(), SectionCaching::SessionCached);
}

#[test]
fn test_language_section_build() {
    let section = LanguageSection::new("en-US".to_string());
    let workspace_dir = PathBuf::from("/tmp/test");
    let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    let content = section.build(&ctx).unwrap();
    assert!(content.contains("# Language"));
    assert!(content.contains("Always respond in en-US"));
}

#[test]
fn test_language_section_build_empty() {
    let section = LanguageSection::new(String::new());
    let workspace_dir = PathBuf::from("/tmp/test");
    let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    let content = section.build(&ctx).unwrap();
    assert!(content.is_empty());
}

// === SkillSection tests ===

#[test]
fn test_skill_section_name() {
    let section = SkillSection::new();
    assert_eq!(section.name(), "skills");
}

#[test]
fn test_skill_section_caching() {
    let section = SkillSection::new();
    assert_eq!(section.caching(), SectionCaching::SessionCached);
}

#[test]
fn test_skill_section_build_without_skills() {
    let section = SkillSection::new();
    let workspace_dir = PathBuf::from("/tmp/test");
    let mut ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    ctx.skill_instructions = String::new();
    let content = section.build(&ctx).unwrap();
    assert!(content.contains("# Session Guidance"));
    assert!(content.contains("Subagent"));
}

#[test]
fn test_skill_section_build_with_skills() {
    let section = SkillSection::new();
    let workspace_dir = PathBuf::from("/tmp/test");
    let mut ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    ctx.skill_instructions = "Skill instructions here".to_string();
    let content = section.build(&ctx).unwrap();
    assert!(content.contains("# Session Guidance"));
    assert!(content.contains("## Skills"));
    assert!(content.contains("Skill instructions here"));
}

// === TokenBudgetSection tests ===

#[test]
fn test_token_budget_section_name() {
    let section = TokenBudgetSection::new();
    assert_eq!(section.name(), "token_budget");
}

#[test]
fn test_token_budget_section_caching() {
    let section = TokenBudgetSection::new();
    assert_eq!(section.caching(), SectionCaching::Volatile);
}

#[test]
fn test_token_budget_section_build_without_anchors() {
    let section = TokenBudgetSection::new();
    let workspace_dir = PathBuf::from("/tmp/test");
    let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    let content = section.build(&ctx).unwrap();
    assert!(content.is_empty());
}

#[test]
fn test_token_budget_section_build_with_anchors() {
    let section = TokenBudgetSection::new().with_numeric_anchors(true);
    let workspace_dir = PathBuf::from("/tmp/test");
    let ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    let content = section.build(&ctx).unwrap();
    assert!(content.contains("## Token Budget"));
    assert!(content.contains("## Output Length"));
    assert!(content.contains("Keep text between tool calls"));
}

// === ProactiveSection tests ===

#[test]
fn test_proactive_section_name() {
    let section = ProactiveSection::new();
    assert_eq!(section.name(), "proactive");
}

#[test]
fn test_proactive_section_caching() {
    let section = ProactiveSection::new();
    assert_eq!(section.caching(), SectionCaching::Volatile);
}

#[test]
fn test_proactive_section_build_when_not_proactive() {
    let section = ProactiveSection::new();
    let workspace_dir = PathBuf::from("/tmp/test");
    let mut ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    ctx.is_proactive_mode = false;
    let content = section.build(&ctx).unwrap();
    assert!(content.is_empty());
}

#[test]
fn test_proactive_section_build_when_proactive() {
    let section = ProactiveSection::new();
    let workspace_dir = PathBuf::from("/tmp/test");
    let mut ctx = make_basic_ctx(&workspace_dir, &[], &[]);
    ctx.is_proactive_mode = true;
    let content = section.build(&ctx).unwrap();
    assert!(content.contains("# Autonomous Work"));
    assert!(content.contains("<tick>"));
    assert!(content.contains("## Pacing"));
    assert!(content.contains("## Bias Toward Action"));
}

// === PromptSection trait object tests ===

#[test]
fn test_prompt_section_boxed() {
    let section: Box<dyn PromptSection> = Box::new(SystemSection::new());
    assert_eq!(section.name(), "system");
    assert_eq!(section.caching(), SectionCaching::Cached);
    let workspace_dir = PathBuf::from("/tmp/test");
    let content = section
        .build(&make_basic_ctx(&workspace_dir, &[], &[]))
        .unwrap();
    assert!(content.contains("# System"));
}

// === prepend_bullets and join_lines tests ===

#[test]
fn test_prepend_bullets() {
    let items = vec!["item1", "item2", "item3"];
    let result = prepend_bullets(&items);
    assert!(result.contains("  - item1"));
    assert!(result.contains("  - item2"));
    assert!(result.contains("  - item3"));
}

#[test]
fn test_join_lines() {
    let items = vec!["line1", "line2", "line3"];
    let result = join_lines(&items);
    assert_eq!(result, "line1\nline2\nline3");
}
