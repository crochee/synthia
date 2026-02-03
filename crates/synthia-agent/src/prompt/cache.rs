use std::collections::HashMap;

use crate::utils::hash::compute_hash;

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

pub struct CacheBreakDetector {
    state_by_source: HashMap<String, TrackedState>,
    max_tracked_sources: usize,
}

impl Default for CacheBreakDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl CacheBreakDetector {
    pub fn new() -> Self {
        Self {
            state_by_source: HashMap::new(),
            max_tracked_sources: 10,
        }
    }

    pub fn record_prompt_state(
        &mut self,
        source: &str,
        snapshot: PromptStateSnapshot,
    ) {
        if self.state_by_source.len() >= self.max_tracked_sources {
            let oldest_key = self.state_by_source.keys().next().cloned();
            if let Some(key) = oldest_key {
                self.state_by_source.remove(&key);
            }
        }

        let state = self.state_by_source.entry(source.to_string()).or_default();

        state.call_count += 1;
        state.prev_model = state.model.clone();
        state.system_hash = snapshot.system_hash;
        state.tools_hash = snapshot.tools_hash;
        state.cache_control_hash = snapshot.cache_control_hash;
        state.tool_names = snapshot.per_tool_hashes.keys().cloned().collect();
        state.per_tool_hashes = snapshot.per_tool_hashes;
        state.system_char_count = snapshot.system_content.len();
        state.model = snapshot.model;
        state.fast_mode = snapshot.fast_mode;
        state.global_cache_strategy = snapshot.global_cache_strategy;
        state.betas = snapshot.betas;
    }

    pub fn check_cache_break(
        &self,
        source: &str,
        cache_read_tokens: u64,
        cache_creation_tokens: u64,
    ) -> Option<CacheBreakReport> {
        let state = self.state_by_source.get(source)?;

        let prev_cache_read = state.prev_cache_read_tokens?;

        if cache_read_tokens >= prev_cache_read {
            return None;
        }

        let token_drop = prev_cache_read - cache_read_tokens;
        if token_drop < 2000 {
            return None;
        }

        let mut report = CacheBreakReport {
            reason: "unknown".to_string(),
            system_prompt_changed: false,
            tool_schemas_changed: false,
            model_changed: false,
            fast_mode_changed: false,
            cache_control_changed: false,
            global_cache_strategy_changed: false,
            betas_changed: false,
            prev_cache_read_tokens: prev_cache_read,
            cache_read_tokens,
            cache_creation_tokens,
            call_count: state.call_count,
        };

        if state.system_hash != 0 {
            report.system_prompt_changed = true;
        }
        if state.tools_hash != 0 {
            report.tool_schemas_changed = true;
        }
        if state.model != state.prev_model {
            report.model_changed = true;
        }
        if state.fast_mode {
            report.fast_mode_changed = true;
        }
        if state.cache_control_hash != 0 {
            report.cache_control_changed = true;
        }
        if !state.global_cache_strategy.is_empty() {
            report.global_cache_strategy_changed = true;
        }
        if !state.betas.is_empty() {
            report.betas_changed = true;
        }

        report.reason = if report.system_prompt_changed {
            "system prompt changed".to_string()
        } else if report.tool_schemas_changed {
            "tool schemas changed".to_string()
        } else if report.model_changed {
            "model changed".to_string()
        } else if report.fast_mode_changed {
            "fast mode changed".to_string()
        } else if report.cache_control_changed {
            "cache control changed".to_string()
        } else if report.global_cache_strategy_changed {
            "global cache strategy changed".to_string()
        } else if report.betas_changed {
            "betas changed".to_string()
        } else {
            "possible TTL expiry".to_string()
        };

        Some(report)
    }

    pub fn notify_cache_deletion(&mut self, source: &str) {
        if let Some(state) = self.state_by_source.get_mut(source) {
            state.cache_deletions_pending = true;
        }
    }

    pub fn notify_compaction(&mut self, source: &str) {
        if let Some(state) = self.state_by_source.get_mut(source) {
            state.prev_cache_read_tokens = None;
        }
    }

    pub fn cleanup_source(&mut self, source: &str) {
        self.state_by_source.remove(source);
    }

    pub fn reset(&mut self) {
        self.state_by_source.clear();
    }

    pub fn get_call_count(&self, source: &str) -> Option<u64> {
        self.state_by_source.get(source).map(|s| s.call_count)
    }
}

pub fn create_prompt_snapshot(
    system_content: &str,
    tools_content: &str,
    model: &str,
    fast_mode: bool,
) -> PromptStateSnapshot {
    let system_hash = compute_hash(system_content);
    let tools_hash = compute_hash(tools_content);
    let cache_control_hash = compute_hash(system_content);
    let prefix_hash = compute_hash(system_content);

    PromptStateSnapshot {
        system_content: system_content.to_string(),
        system_hash,
        tools_hash,
        per_tool_hashes: HashMap::new(),
        cache_control_hash,
        model: model.to_string(),
        fast_mode,
        betas: Vec::new(),
        global_cache_strategy: String::new(),
        timestamp: chrono::Utc::now().timestamp(),
        volatile_sections: Vec::new(),
        prefix_hash: format!("{prefix_hash:x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_break_detector_new() {
        let detector = CacheBreakDetector::new();
        assert!(detector.state_by_source.is_empty());
    }

    #[test]
    fn test_record_prompt_state() {
        let mut detector = CacheBreakDetector::new();
        let snapshot =
            create_prompt_snapshot("system", "tools", "claude-3", false);
        detector.record_prompt_state("test_source", snapshot);

        let count = detector.get_call_count("test_source");
        assert_eq!(count, Some(1));
    }

    #[test]
    fn test_record_prompt_state_increments_call_count() {
        let mut detector = CacheBreakDetector::new();
        let snapshot1 =
            create_prompt_snapshot("system", "tools", "claude-3", false);
        detector.record_prompt_state("test_source", snapshot1);

        let snapshot2 =
            create_prompt_snapshot("system2", "tools2", "claude-3", false);
        detector.record_prompt_state("test_source", snapshot2);

        let count = detector.get_call_count("test_source");
        assert_eq!(count, Some(2));
    }

    #[test]
    fn test_cache_break_detection_no_break() {
        let mut detector = CacheBreakDetector::new();
        let snapshot =
            create_prompt_snapshot("system", "tools", "claude-3", false);
        detector.record_prompt_state("test_source", snapshot);

        let result = detector.check_cache_break("test_source", 10000, 5000);
        assert!(result.is_none());
    }

    #[test]
    fn test_notify_cache_deletion() {
        let mut detector = CacheBreakDetector::new();
        let snapshot =
            create_prompt_snapshot("system", "tools", "claude-3", false);
        detector.record_prompt_state("test_source", snapshot);

        detector.notify_cache_deletion("test_source");

        let state = detector.state_by_source.get("test_source");
        assert!(state.unwrap().cache_deletions_pending);
    }

    #[test]
    fn test_notify_compaction() {
        let mut detector = CacheBreakDetector::new();
        let snapshot =
            create_prompt_snapshot("system", "tools", "claude-3", false);
        detector.record_prompt_state("test_source", snapshot);

        detector.notify_compaction("test_source");

        let state = detector.state_by_source.get("test_source");
        assert!(state.unwrap().prev_cache_read_tokens.is_none());
    }

    #[test]
    fn test_cleanup_source() {
        let mut detector = CacheBreakDetector::new();
        let snapshot =
            create_prompt_snapshot("system", "tools", "claude-3", false);
        detector.record_prompt_state("test_source", snapshot);

        detector.cleanup_source("test_source");

        assert!(detector.get_call_count("test_source").is_none());
    }

    #[test]
    fn test_reset() {
        let mut detector = CacheBreakDetector::new();
        let snapshot =
            create_prompt_snapshot("system", "tools", "claude-3", false);
        detector.record_prompt_state("test_source", snapshot);

        detector.reset();

        assert!(detector.state_by_source.is_empty());
    }

    #[test]
    fn test_create_prompt_snapshot() {
        let snapshot = create_prompt_snapshot(
            "system content",
            "tools content",
            "claude-3",
            true,
        );

        assert_eq!(snapshot.model, "claude-3");
        assert!(snapshot.fast_mode);
        assert!(snapshot.system_hash != 0);
        assert!(snapshot.tools_hash != 0);
    }
}
