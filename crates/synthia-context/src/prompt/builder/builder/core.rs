//! The [`PromptBuilder`] struct itself, plus the `new` / `add_section`
//! / `section_names` / `get_static_sections` / `get_dynamic_sections`
//! methods and the `Debug` / `Default` impls.

use crate::prompt::PromptSection;

/// Section orchestrator. Walks a `Vec<Box<dyn PromptSection>>`,
/// classifies each into static / dynamic / volatile / uncached via
/// [`crate::prompt::section_trait::SectionCaching`], and emits
/// [`super::super::state::ResolvedPrompt`] with a prefix-hash
/// suitable for KV-cache stability tracking.
pub struct PromptBuilder {
    pub(crate) sections: Vec<Box<dyn PromptSection>>,
}

impl std::fmt::Debug for PromptBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PromptBuilder")
            .field("sections_count", &self.sections.len())
            .finish_non_exhaustive()
    }
}

impl Default for PromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptBuilder {
    /// Empty builder. Callers add sections via [`Self::add_section`]
    /// or use one of the presets in [`super::defaults`].
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
        }
    }

    /// Append a section to the section list. Consuming builder.
    pub fn add_section(mut self, section: Box<dyn PromptSection>) -> Self {
        self.sections.push(section);
        self
    }

    /// Names of every section in the order they were added.
    pub fn section_names(&self) -> Vec<&str> {
        self.sections
            .iter()
            .map(crate::prompt::PromptSection::name)
            .collect()
    }

    /// Names of sections cached for the full session lifetime
    /// (KV-cache prefix-stable).
    pub fn get_static_sections(&self) -> Vec<&str> {
        self.sections
            .iter()
            .filter(|s| {
                s.caching()
                    == crate::prompt::section_trait::SectionCaching::Cached
            })
            .map(crate::prompt::PromptSection::name)
            .collect()
    }

    /// Names of sections that are NOT cached for the full session
    /// (i.e. SessionCached, Volatile, Uncached).
    pub fn get_dynamic_sections(&self) -> Vec<&str> {
        self.sections
            .iter()
            .filter(|s| {
                s.caching()
                    != crate::prompt::section_trait::SectionCaching::Cached
            })
            .map(crate::prompt::PromptSection::name)
            .collect()
    }
}
