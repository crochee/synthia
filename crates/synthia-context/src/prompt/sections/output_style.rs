use anyhow::Result;

use super::PromptSection;
use crate::prompt::SectionCaching;

#[derive(Debug, Clone)]
pub struct OutputStyleSection {
    name: String,
    prompt: String,
}

impl OutputStyleSection {
    pub fn new(name: String, prompt: String) -> Self {
        Self { name, prompt }
    }
}

impl Default for OutputStyleSection {
    fn default() -> Self {
        Self::new(
            std::env::var("OUTPUT_STYLE_NAME").unwrap_or_default(),
            std::env::var("OUTPUT_STYLE_PROMPT").unwrap_or_default(),
        )
    }
}

impl PromptSection for OutputStyleSection {
    fn name(&self) -> &str {
        "output_style"
    }

    fn caching(&self) -> SectionCaching {
        SectionCaching::SessionCached
    }

    fn build(&self, _ctx: &super::super::PromptContext<'_>) -> Result<String> {
        if self.prompt.is_empty() {
            return Ok(String::new());
        }
        Ok(format!("# Output Style: {}\n{}", self.name, self.prompt))
    }
}
