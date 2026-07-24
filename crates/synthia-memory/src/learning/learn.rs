//! The 2 learn methods on [`super::core::ExperienceLearner`]:
//!
//! - [`ExperienceLearner::learn_from_success`] — adds a
//!   `PatternType::Success` pattern and either increments
//!   the existing `LearnedExperience::success_count` (and
//!   exponentially-decay-blends the confidence) or creates
//!   a new `LearnedExperience` with `success_count = 1`.
//! - [`ExperienceLearner::learn_from_failure`] — same
//!   pattern for failures (mirrors success: `failure_count`
//!   increment, confidence blended against
//!   `1.0 - quality_score`).
//!
//! Both use `exp_{task_type}` as the experience ID
//! (one per `task_type`).

use chrono::Utc;

use super::{
    core::ExperienceLearner,
    learned::LearnedExperience,
    types::{ExperienceRecord, Pattern, PatternType},
};

impl ExperienceLearner {
    pub fn learn_from_success(&mut self, record: &ExperienceRecord) {
        let experience_id = format!("exp_{}", record.task_type);
        let pattern = Pattern {
            pattern_type: PatternType::Success,
            description: record.outcome.result_summary.clone(),
            confidence: record.outcome.quality_score,
            applies_to: vec![record.task_type.clone()],
        };

        self.success_patterns.push(pattern);

        if let Some(existing) =
            self.experiences.iter_mut().find(|e| e.id == experience_id)
        {
            existing.success_count += 1;
            existing.confidence = (existing.confidence * 0.9
                + record.outcome.quality_score * 0.1)
                .min(1.0);
            existing.last_applied = Some(Utc::now());
        } else {
            self.experiences.push(LearnedExperience {
                id: experience_id,
                pattern_type: PatternType::Success,
                description: record.outcome.result_summary.clone(),
                success_count: 1,
                failure_count: 0,
                confidence: record.outcome.quality_score,
                last_applied: Some(Utc::now()),
                related_contexts: vec![record.task_type.clone()],
            });
        }
    }

    pub fn learn_from_failure(&mut self, record: &ExperienceRecord) {
        let experience_id = format!("exp_{}", record.task_type);
        let pattern = Pattern {
            pattern_type: PatternType::Failure,
            description: record
                .outcome
                .error_message
                .clone()
                .unwrap_or_default(),
            confidence: 1.0 - record.outcome.quality_score,
            applies_to: vec![record.task_type.clone()],
        };

        self.failure_patterns.push(pattern);

        if let Some(existing) =
            self.experiences.iter_mut().find(|e| e.id == experience_id)
        {
            existing.failure_count += 1;
            existing.confidence = (existing.confidence * 0.9
                + (1.0 - record.outcome.quality_score) * 0.1)
                .min(1.0);
        } else {
            self.experiences.push(LearnedExperience {
                id: experience_id,
                pattern_type: PatternType::Failure,
                description: record
                    .outcome
                    .error_message
                    .clone()
                    .unwrap_or_default(),
                success_count: 0,
                failure_count: 1,
                confidence: 1.0 - record.outcome.quality_score,
                last_applied: None,
                related_contexts: vec![record.task_type.clone()],
            });
        }
    }
}
