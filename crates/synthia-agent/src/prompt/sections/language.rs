use super::PromptSection;
use crate::prompt::{Result, SectionCaching};

#[derive(Debug, Clone)]
pub struct LanguageSection {
    language: String,
}

impl LanguageSection {
    pub fn new(language: String) -> Self {
        Self { language }
    }
}

impl Default for LanguageSection {
    fn default() -> Self {
        Self::new(
            sys_locale::get_locale().unwrap_or_else(|| String::from("en-US")),
        )
    }
}

impl PromptSection for LanguageSection {
    fn name(&self) -> &str {
        "language"
    }

    fn caching(&self) -> SectionCaching {
        SectionCaching::SessionCached
    }

    fn build(&self, _ctx: &crate::prompt::PromptContext<'_>) -> Result<String> {
        if self.language.is_empty() {
            return Ok(String::new());
        }
        Ok(format!(
            "# Language\nAlways respond in {}. Use {} for all explanations, comments, and communications with the user. Technical terms and code identifiers should remain in their original form.",
            self.language, self.language
        ))
    }
}
