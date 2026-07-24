use anyhow::Result;

use super::{PromptSection, prepend_bullets};
use crate::prompt::{PromptContext, SectionCaching};

const TASK_ITEMS: &[&str] = &[
    "Bug fixes, features, refactoring, code explanation.",
    "Read before modifying. Verify before assuming.",
    "No unnecessary files. Edit existing over create new.",
    "Diagnose failures before changing approach.",
    "Avoid security vulnerabilities (injection, XSS, SQL).",
    "If you cannot verify something, say so clearly.",
];

const CODE_STYLE_ITEMS: &[&str] = &[
    "No gold-plating. No premature abstraction.",
    "No comments unless WHY is non-obvious.",
    "No error handling for impossible scenarios.",
    "Use meaningful names. Prefer clarity over brevity.",
    "Keep functions small. Single responsibility.",
    "Verify before claiming: run tests, check output.",
];

const ACTION_ITEMS: &[&str] = &[
    "**Freely take**: Local reversible actions (edits, tests).",
    "**Confirm first**: Risky/destructive actions (delete, force-push).",
    "When blocked, do NOT use destructive shortcuts.",
];

#[derive(Debug, Clone, Copy, Default)]
pub struct TaskExecutionSection;

impl TaskExecutionSection {
    pub fn new() -> Self {
        Self
    }
}

impl PromptSection for TaskExecutionSection {
    fn name(&self) -> &str {
        "task_execution"
    }

    fn caching(&self) -> SectionCaching {
        SectionCaching::Cached
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        let subagent_items = if ctx.is_subagent {
            vec![
                "Stay within scope of assigned task.",
                "Use minimum necessary tools.",
                "Report completion with file paths and key findings.",
            ]
        } else {
            Vec::new()
        };

        let subagent_block = if subagent_items.is_empty() {
            String::new()
        } else {
            format!("\n## Subagent\n{}\n", prepend_bullets(&subagent_items))
        };

        Ok(format!(
            "# Doing tasks\n\n## Task Guidelines\n{}\n\n## Code Style\n{}\n\n## Action Guidelines\n{}\n{}{subagent_block}",
            prepend_bullets(TASK_ITEMS),
            prepend_bullets(CODE_STYLE_ITEMS),
            prepend_bullets(ACTION_ITEMS),
            if ctx.is_subagent { "" } else { "\n" },
        ))
    }
}
