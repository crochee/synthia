use super::{Source, SourceContent, SourceDelta, SourceId};

/// Tracks system prompt text content.
pub struct SystemPromptSource {
    id: SourceId,
    baseline_content: SourceContent,
    current_text: String,
}

impl SystemPromptSource {
    /// Create a new source with the given initial text as the baseline.
    pub fn new(text: String) -> Self {
        let baseline_content = SourceContent::from_text(&text);
        Self {
            id: SourceId("system-prompt"),
            baseline_content,
            current_text: text,
        }
    }

    /// Update the current text. Call [`update`](Source::update) afterwards to
    /// get the delta.
    pub fn set_text(&mut self, text: String) {
        self.current_text = text;
    }
}

impl Source for SystemPromptSource {
    fn id(&self) -> SourceId {
        self.id.clone()
    }

    fn baseline(&self) -> SourceContent {
        self.baseline_content.clone()
    }

    fn update(&mut self) -> SourceDelta {
        let new_content = SourceContent::from_text(&self.current_text);
        if new_content.hash() == self.baseline_content.hash() {
            SourceDelta::Unchanged
        } else {
            SourceDelta::Changed(new_content)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_is_system_prompt() {
        let source = SystemPromptSource::new("hello".to_string());
        assert_eq!(source.id(), SourceId("system-prompt"));
    }

    #[test]
    fn update_returns_unchanged_for_same_text() {
        let mut source = SystemPromptSource::new("hello".to_string());
        assert!(matches!(source.update(), SourceDelta::Unchanged));
    }

    #[test]
    fn update_returns_changed_for_new_text() {
        let mut source = SystemPromptSource::new("hello".to_string());
        source.set_text("world".to_string());
        assert!(matches!(source.update(), SourceDelta::Changed(_)));
    }

    #[test]
    fn update_returns_unchanged_when_reset_to_baseline() {
        let mut source = SystemPromptSource::new("hello".to_string());
        source.set_text("world".to_string());
        assert!(matches!(source.update(), SourceDelta::Changed(_)));
        source.set_text("hello".to_string());
        assert!(matches!(source.update(), SourceDelta::Unchanged));
    }

    #[test]
    fn baseline_reflects_initial_text() {
        let source = SystemPromptSource::new("initial".to_string());
        let baseline = source.baseline();
        assert_eq!(baseline.0, b"initial");
    }
}
