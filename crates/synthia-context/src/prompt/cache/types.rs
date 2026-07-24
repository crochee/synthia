use std::{collections::HashMap, hash::Hasher};

use crate::source::{SourceEpoch, SourceId};

pub(super) fn compute_hash(content: &str) -> u64 {
    let mut hasher = ahash::AHasher::default();
    hasher.write(content.as_bytes());
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::compute_hash;

    #[test]
    fn test_compute_hash_deterministic() {
        assert_eq!(compute_hash("abc"), compute_hash("abc"));
    }

    #[test]
    fn test_compute_hash_differs_for_different_content() {
        assert_ne!(compute_hash("abc"), compute_hash("abd"));
    }
}

#[derive(Debug, Clone)]
pub struct PromptStateSnapshot {
    pub system_content: String,
    pub system_hash: u64,
    pub tools_hash: u64,
    pub per_tool_hashes: HashMap<String, u64>,
    pub cache_control_hash: u64,
    pub model: String,
    pub fast_mode: bool,
    pub betas: Vec<String>,
    pub global_cache_strategy: String,
    pub timestamp: i64,
    pub volatile_sections: Vec<String>,
    pub prefix_hash: String,
}

impl Default for PromptStateSnapshot {
    fn default() -> Self {
        Self {
            system_content: String::new(),
            system_hash: 0,
            tools_hash: 0,
            per_tool_hashes: HashMap::new(),
            cache_control_hash: 0,
            model: String::new(),
            fast_mode: false,
            betas: Vec::new(),
            global_cache_strategy: String::new(),
            timestamp: chrono::Utc::now().timestamp(),
            volatile_sections: Vec::new(),
            prefix_hash: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TrackedState {
    pub system_hash: u64,
    pub tools_hash: u64,
    pub cache_control_hash: u64,
    pub sources: HashMap<SourceId, SourceEpoch>,
    pub tool_names: Vec<String>,
    pub per_tool_hashes: HashMap<String, u64>,
    pub system_char_count: usize,
    pub model: String,
    pub prev_model: String,
    pub fast_mode: bool,
    pub global_cache_strategy: String,
    pub betas: Vec<String>,
    pub call_count: u64,
    pub prev_cache_read_tokens: Option<u64>,
    pub cache_deletions_pending: bool,
}

#[derive(Debug, Clone)]
pub struct CacheBreakReport {
    pub reason: String,
    pub system_prompt_changed: bool,
    pub tool_schemas_changed: bool,
    pub model_changed: bool,
    pub fast_mode_changed: bool,
    pub cache_control_changed: bool,
    pub global_cache_strategy_changed: bool,
    pub betas_changed: bool,
    pub prev_cache_read_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub call_count: u64,
}
