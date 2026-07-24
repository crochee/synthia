use anyhow::Result;

use super::{PromptSection, inject_workspace_file};
use crate::prompt::{PromptContext, SectionCaching};

const MEMORY_FILES: &[&str] =
    &["MEMORY.md", "CONTEXT.md", "NOTES.md", "WORKSPACE.md"];

#[derive(Debug, Clone, Default)]
pub struct MemorySection;

impl MemorySection {
    pub fn new() -> Self {
        Self
    }
}

impl PromptSection for MemorySection {
    fn name(&self) -> &str {
        "memory"
    }

    fn caching(&self) -> SectionCaching {
        SectionCaching::SessionCached
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        let mut has_memory = false;
        let mut output = String::new();

        for file in MEMORY_FILES {
            let path = ctx.workspace_dir.join(format!(".agents/{file}"));
            if path.exists()
                && let Ok(content) = std::fs::read_to_string(&path)
            {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    has_memory = true;
                    inject_workspace_file(&mut output, ctx.workspace_dir, file);
                }
            }
        }

        if has_memory {
            Ok(output)
        } else {
            Ok(String::new())
        }
    }
}
