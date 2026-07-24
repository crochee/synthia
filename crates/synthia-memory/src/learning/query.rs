//! The 3 query methods on
//! [`super::core::ExperienceLearner`]:
//!
//! - [`ExperienceLearner::suggest_action`] — returns up to
//!   5 [`ActionSuggestion`]s sorted by confidence
//!   (descending), with `FollowPattern` for high-confidence
//!   Success experiences and `AvoidPattern` for
//!   low-success-rate Failure experiences.
//! - [`ExperienceLearner::get_reliable_patterns`] — pure
//!   read accessor that filters by
//!   [`LearnedExperience::is_reliable`] (confidence > 0.7
//!   and `success_count >= 3`).
//! - [`ExperienceLearner::get_failure_warnings`] — pure
//!   read accessor that filters by `pattern_type ==
//!   Failure` and `success_rate < 0.5`.

use super::{
    core::ExperienceLearner,
    learned::{ActionSuggestion, LearnedExperience, SuggestionType},
    types::PatternType,
};

impl ExperienceLearner {
    pub fn suggest_action(&self, _context: &str) -> Vec<ActionSuggestion> {
        let mut suggestions = Vec::new();

        for exp in &self.experiences {
            if exp.is_reliable() {
                if exp.pattern_type == PatternType::Success {
                    suggestions.push(ActionSuggestion {
                        action_type: SuggestionType::FollowPattern,
                        description: exp.description.clone(),
                        confidence: exp.confidence,
                        reason: format!(
                            "This pattern succeeded {} times ({}% success rate)",
                            exp.success_count,
                            (exp.success_rate() * 100.0) as usize
                        ),
                    });
                } else if exp.pattern_type == PatternType::Failure
                    && exp.success_rate() < 0.3
                {
                    suggestions.push(ActionSuggestion {
                        action_type: SuggestionType::AvoidPattern,
                        description: exp.description.clone(),
                        confidence: exp.confidence,
                        reason: format!(
                            "This pattern failed {} times ({}% success rate)",
                            exp.failure_count,
                            (exp.success_rate() * 100.0) as usize
                        ),
                    });
                }
            }
        }

        suggestions
            .sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
        suggestions.truncate(5);
        suggestions
    }

    pub fn get_reliable_patterns(&self) -> Vec<&LearnedExperience> {
        self.experiences
            .iter()
            .filter(|e| e.is_reliable())
            .collect()
    }

    pub fn get_failure_warnings(&self) -> Vec<&LearnedExperience> {
        self.experiences
            .iter()
            .filter(|e| {
                e.pattern_type == PatternType::Failure && e.success_rate() < 0.5
            })
            .collect()
    }
}
