use super::{Source, SourceContent, SourceDelta, SourceId};

/// Tracks the skill identifier list contributing to the prompt prefix.
///
/// Initial implementation: [`update`](Source::update) always reports
/// [`SourceDelta::Unchanged`]. Skill-provider delta tracking is deferred to a
/// separate change.
pub struct SkillListSource {
    id: SourceId,
    baseline_content: SourceContent,
}

impl SkillListSource {
    /// Create a new source with the given skill ids as the baseline. The ids
    /// are sorted and joined to produce a canonical representation.
    pub fn new(skill_ids: Vec<String>) -> Self {
        let mut sorted = skill_ids;
        sorted.sort();
        let joined = sorted.join(",");
        Self {
            id: SourceId("skill-list"),
            baseline_content: SourceContent::from_text(&joined),
        }
    }
}

impl Source for SkillListSource {
    fn id(&self) -> SourceId {
        self.id.clone()
    }

    fn baseline(&self) -> SourceContent {
        self.baseline_content.clone()
    }

    fn update(&mut self) -> SourceDelta {
        SourceDelta::Unchanged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_is_skill_list() {
        let source = SkillListSource::new(vec!["a".to_string()]);
        assert_eq!(source.id(), SourceId("skill-list"));
    }

    #[test]
    fn update_always_unchanged() {
        let mut source =
            SkillListSource::new(vec!["a".to_string(), "b".to_string()]);
        assert!(matches!(source.update(), SourceDelta::Unchanged));
    }

    #[test]
    fn canonical_order_is_independent_of_input_order() {
        let a = SkillListSource::new(vec!["a".to_string(), "b".to_string()]);
        let b = SkillListSource::new(vec!["b".to_string(), "a".to_string()]);
        assert_eq!(a.baseline().hash(), b.baseline().hash());
    }
}
