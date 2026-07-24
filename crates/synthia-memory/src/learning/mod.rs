//! Experience Learning — enables agents to learn from
//! successes and failures, building up patterns that improve
//! future task execution.
//!
//! # Module Layout
//!
//! - [`types`]: The 5 input/output data records
//!   ([`types::ExperienceRecord`] + `new`,
//!   [`types::TaskContext`], [`types::Outcome`],
//!   [`types::Pattern`], [`types::PatternType`]).
//! - [`learned`]: The 4 derived data records
//!   ([`learned::LearnedExperience`] + 2 methods,
//!   [`learned::ActionSuggestion`],
//!   [`learned::SuggestionType`], [`learned::LearningReport`]).
//! - [`core`]: [`core::ExperienceLearner`] struct +
//!   `Default` (the in-memory state container).
//! - [`learn`]: The 2 learn methods
//!   ([`learn::ExperienceLearner::learn_from_success`],
//!   `learn_from_failure`).
//! - [`query`]: The 3 query methods
//!   ([`query::ExperienceLearner::suggest_action`],
//!   `get_reliable_patterns`, `get_failure_warnings`).
//! - [`persistence`]: The 4 persistence methods
//!   ([`persistence::ExperienceLearner::serialize`],
//!   `deserialize`, `save_to_file`, `load_from_file`).
//! - [`report`]: The `generate_report` method (returns a
//!   [`learned::LearningReport`]).
//! - [`tests`]: 7 unit tests.

mod core;
mod learn;
mod learned;
mod persistence;
mod query;
mod report;
mod types;

#[cfg(test)]
mod tests;

pub use core::ExperienceLearner;

pub use learned::{
    ActionSuggestion,
    LearnedExperience,
    LearningReport,
    SuggestionType,
};
pub use types::{ExperienceRecord, Outcome, Pattern, PatternType, TaskContext};
