//! Per-source epoch tracking for cache break detection.

use super::{SourceContent, SourceDelta};

/// Tracks baseline vs current hash for a single source.
#[derive(Debug, Clone)]
pub struct SourceEpoch {
    baseline_hash: u64,
    current_hash: u64,
    content: SourceContent,
    removed: bool,
}

impl SourceEpoch {
    /// Construct from initial content. `baseline_hash == current_hash`.
    pub fn new(content: SourceContent) -> Self {
        let hash = content.hash();
        Self {
            baseline_hash: hash,
            current_hash: hash,
            content,
            removed: false,
        }
    }

    /// True when `baseline_hash != current_hash` and not removed.
    pub fn is_changed(&self) -> bool {
        !self.removed && self.baseline_hash != self.current_hash
    }

    /// True when the source returned `Removed` delta.
    pub fn is_removed(&self) -> bool {
        self.removed
    }

    /// Apply a delta, updating current state.
    pub fn apply_delta(&mut self, delta: SourceDelta) {
        match delta {
            SourceDelta::Changed(new_content) => {
                self.current_hash = new_content.hash();
                self.content = new_content;
            }
            SourceDelta::Unchanged => {}
            SourceDelta::Removed => {
                self.removed = true;
            }
        }
    }

    /// Current content reference.
    pub fn content(&self) -> &SourceContent {
        &self.content
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_not_changed() {
        let epoch = SourceEpoch::new(SourceContent::from_text("hello"));
        assert!(!epoch.is_changed());
        assert!(!epoch.is_removed());
    }

    #[test]
    fn changed_delta_marks_changed() {
        let mut epoch = SourceEpoch::new(SourceContent::from_text("hello"));
        assert!(!epoch.is_changed());
        epoch.apply_delta(SourceDelta::Changed(SourceContent::from_text(
            "world",
        )));
        assert!(epoch.is_changed());
        assert!(!epoch.is_removed());
    }

    #[test]
    fn unchanged_delta_preserves_state() {
        let mut epoch = SourceEpoch::new(SourceContent::from_text("hello"));
        epoch.apply_delta(SourceDelta::Changed(SourceContent::from_text(
            "world",
        )));
        assert!(epoch.is_changed());
        // Unchanged delta must not alter the recorded change.
        epoch.apply_delta(SourceDelta::Unchanged);
        assert!(epoch.is_changed());
    }

    #[test]
    fn removed_delta_marks_removed() {
        let mut epoch = SourceEpoch::new(SourceContent::from_text("hello"));
        epoch.apply_delta(SourceDelta::Removed);
        assert!(epoch.is_removed());
        // Removed sources are not reported as "changed".
        assert!(!epoch.is_changed());
    }

    #[test]
    fn same_content_after_changed_is_not_changed() {
        let mut epoch = SourceEpoch::new(SourceContent::from_text("hello"));
        // Applying a delta with identical content keeps baseline == current.
        epoch.apply_delta(SourceDelta::Changed(SourceContent::from_text(
            "hello",
        )));
        assert!(!epoch.is_changed());
    }

    #[test]
    fn empty_content_does_not_false_report() {
        // Even when content is empty, baseline == current so no false report.
        let epoch = SourceEpoch::new(SourceContent(Vec::new()));
        assert!(!epoch.is_changed());
    }

    #[test]
    fn content_returns_current() {
        let mut epoch = SourceEpoch::new(SourceContent::from_text("hello"));
        epoch.apply_delta(SourceDelta::Changed(SourceContent::from_text(
            "world",
        )));
        assert_eq!(epoch.content().0, b"world");
    }
}
