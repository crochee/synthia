use anyhow::Result;

use super::PromptSection;
use crate::prompt::{PromptContext, SectionCaching};

const SUBAGENT_GUIDANCE: &str = r#"## Subagent

Use Agent tool for delegable tasks with clear scope:

**Explore**: Map codebase structure, find relevant files, understand architecture.
**Verify**: Run tests, validate changes, check for regressions.
**Custom**: Any task with specific instructions and expected output.

Brief subagents like a colleague: what you're accomplishing, what you've learned, and exact scope. Never delegate understanding. Simple searches → use Grep/Glob directly."#;

const SKILLS_GUIDANCE: &str = r#"## Skills

Skill commands (e.g., /commit) are shorthand for invoking user skills. When executed, the skill expands to a full prompt. Use the SkillTool to execute skills. Only use SkillTool for skills listed in its user-invocable skills section — don't guess or use built-in CLI commands."#;

#[derive(Debug, Clone, Default)]
pub struct SkillSection;

impl SkillSection {
    pub fn new() -> Self {
        Self
    }
}

impl PromptSection for SkillSection {
    fn name(&self) -> &str {
        "skills"
    }

    fn caching(&self) -> SectionCaching {
        SectionCaching::SessionCached
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        let mut parts: Vec<&str> = Vec::new();

        parts.push(SUBAGENT_GUIDANCE);
        parts.push("If you don't understand why user denied a tool call, use the AskUser tool to ask.");

        if !ctx.skill_instructions.is_empty() {
            parts.push(SKILLS_GUIDANCE);
            parts.push(&ctx.skill_instructions);
        }

        if parts.is_empty() {
            return Ok(String::new());
        }

        Ok(format!("# Session Guidance\n\n{}", parts.join("\n\n")))
    }
}
