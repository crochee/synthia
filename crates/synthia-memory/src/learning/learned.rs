//! The 4 derived data records returned by the learning
//! system:
//!
//! - [`LearnedExperience`] — the aggregated experience
//!   record (one per `task_type`) with success/failure
//!   counts and confidence.
//! - [`ActionSuggestion`] — a single suggestion
//!   (FollowPattern / AvoidPattern / Optimize / Retry).
//! - [`SuggestionType`] — the 4-variant enum.
//! - [`LearningReport`] — the snapshot of the learner
//!   state (used by [`super::report::ExperienceLearner::generate_report`]).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::types::PatternType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedExperience {
    pub id: String,
    pub pattern_type: PatternType,
    pub description: String,
    pub success_count: usize,
    pub failure_count: usize,
    pub confidence: f64,
    pub last_applied: Option<DateTime<Utc>>,
    pub related_contexts: Vec<String>,
}

impl LearnedExperience {
    pub fn success_rate(&self) -> f64 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            return 0.5;
        }
        self.success_count as f64 / total as f64
    }

    pub fn is_reliable(&self) -> bool {
        self.confidence > 0.7 && self.success_count >= 3
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSuggestion {
    pub action_type: SuggestionType,
    pub description: String,
    pub confidence: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionType {
    FollowPattern,
    AvoidPattern,
    Optimize,
    Retry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningReport {
    pub total_experiences: usize,
    pub reliable_patterns: usize,
    pub failure_warnings: usize,
    pub avg_success_rate: f64,
    pub top_patterns: Vec<LearnedExperience>,
}
