//! The `generate_report` method on
//! [`super::core::ExperienceLearner`] — returns a
//! [`super::learned::LearningReport`] snapshot of the
//! current state (total / reliable / warnings /
//! avg_success_rate / top-10 by confidence).

use super::{core::ExperienceLearner, learned::LearningReport};

impl ExperienceLearner {
    pub fn generate_report(&self) -> LearningReport {
        let total = self.experiences.len();
        let reliable = self.get_reliable_patterns();
        let warnings = self.get_failure_warnings();

        let avg_success = if self.experiences.is_empty() {
            0.5
        } else {
            self.experiences
                .iter()
                .map(|e| e.success_rate())
                .sum::<f64>()
                / total as f64
        };

        let mut top: Vec<_> = self.experiences.clone();
        top.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
        top.truncate(10);

        LearningReport {
            total_experiences: total,
            reliable_patterns: reliable.len(),
            failure_warnings: warnings.len(),
            avg_success_rate: avg_success,
            top_patterns: top,
        }
    }
}
