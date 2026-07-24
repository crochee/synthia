//! The [`ExperienceLearner`] struct + `Default` impl (the
//! in-memory state container).
//!
//! All 10 methods are split across:
//!
//! - [`super::learn`] (2 methods: `learn_from_success` /
//!   `learn_from_failure`).
//! - [`super::query`] (3 methods: `suggest_action` /
//!   `get_reliable_patterns` / `get_failure_warnings`).
//! - [`super::persistence`] (4 methods: `serialize` /
//!   `deserialize` / `save_to_file` / `load_from_file`).
//! - [`super::report`] (1 method: `generate_report`).

use std::collections::HashMap;

use super::{learned::LearnedExperience, types::Pattern};

pub struct ExperienceLearner {
    pub(super) experiences: Vec<LearnedExperience>,
    pub(super) success_patterns: Vec<Pattern>,
    pub(super) failure_patterns: Vec<Pattern>,
    _context_embeddings: HashMap<String, Vec<f32>>,
}

impl ExperienceLearner {
    pub fn new() -> Self {
        Self {
            experiences: Vec::new(),
            success_patterns: Vec::new(),
            failure_patterns: Vec::new(),
            _context_embeddings: HashMap::new(),
        }
    }
}

impl Default for ExperienceLearner {
    fn default() -> Self {
        Self::new()
    }
}
