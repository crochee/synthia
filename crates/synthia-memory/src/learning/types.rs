//! The 5 input/output data records carried by the learning
//! system:
//!
//! - [`ExperienceRecord`] — the per-task input record
//!   (one call to `learn_from_*`).
//! - [`TaskContext`] — the input summary, tools used, step
//!   count, execution time, and environment map.
//! - [`Outcome`] — the result summary, quality score
//!   (0.0-1.0), and optional error type/message.
//! - [`Pattern`] — a learned pattern with confidence and
//!   applicability.
//! - [`PatternType`] — the 4-variant enum (Success /
//!   Failure / Optimization / Warning).

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperienceRecord {
    pub id: String,
    pub task_type: String,
    pub context: TaskContext,
    pub outcome: Outcome,
    pub patterns: Vec<Pattern>,
    pub timestamp: DateTime<Utc>,
    pub success: bool,
}

impl ExperienceRecord {
    pub fn new(
        id: String,
        task_type: String,
        context: TaskContext,
        outcome: Outcome,
        patterns: Vec<Pattern>,
        success: bool,
    ) -> Self {
        Self {
            id,
            task_type,
            context,
            outcome,
            patterns,
            timestamp: Utc::now(),
            success,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub input_summary: String,
    pub tools_used: Vec<String>,
    pub steps_taken: usize,
    pub execution_time_ms: u64,
    pub environment: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    pub result_summary: String,
    pub quality_score: f64,
    pub error_type: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub pattern_type: PatternType,
    pub description: String,
    pub confidence: f64,
    pub applies_to: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    Success,
    Failure,
    Optimization,
    Warning,
}
