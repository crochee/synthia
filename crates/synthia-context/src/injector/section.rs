/// A section of context content with a title, content, and priority score.
///
/// Priority determines the order in which sections are removed during
/// token budget trimming: lowest priority sections are removed first.
#[derive(Debug, Clone)]
pub struct Section {
    pub title: String,
    pub content: String,
    pub priority: u8,
}

impl Section {
    /// Create a new section with the given title, content, and priority.
    pub fn new(
        title: impl Into<String>,
        content: impl Into<String>,
        priority: u8,
    ) -> Self {
        Self {
            title: title.into(),
            content: content.into(),
            priority,
        }
    }

    /// Create a section with the highest priority (100).
    pub fn critical(
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self::new(title, content, 100)
    }

    /// Returns the estimated token count of this section's content using the provided counter.
    pub fn token_count(&self, counter: impl Fn(&str) -> usize) -> usize {
        counter(&self.content)
    }
}
