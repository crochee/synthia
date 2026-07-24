use std::collections::HashMap;

use super::types::{CacheBreakReport, PromptStateSnapshot, TrackedState};
use crate::source::{SourceContent, SourceDelta, SourceEpoch, SourceId};

pub struct CacheBreakDetector {
    pub(super) state_by_source: HashMap<String, TrackedState>,
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

        // Track per-source epochs for cache-break diagnosis. Each source
        // records its baseline on first observation and updates the current
        // hash on subsequent calls, so `is_changed()` reflects divergence from
        // the epoch baseline rather than a fragile non-zero check.
        Self::upsert_source_epoch(
            &mut state.sources,
            SourceId("system-prompt"),
            SourceContent::from_text(&snapshot.system_content),
        );
        Self::upsert_source_epoch(
            &mut state.sources,
            SourceId("tool-schemas"),
            SourceContent(snapshot.tools_hash.to_le_bytes().to_vec()),
        );
        Self::upsert_source_epoch(
            &mut state.sources,
            SourceId("cache-control"),
            SourceContent(snapshot.cache_control_hash.to_le_bytes().to_vec()),
        );
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

        // Diagnose which prefix sources diverged from their epoch baseline.
        // The previous `if hash != 0` checks were always-true after any
        // `record_prompt_state`, producing false positives on every break.
        if state
            .sources
            .get(&SourceId("system-prompt"))
            .is_some_and(SourceEpoch::is_changed)
        {
            report.system_prompt_changed = true;
        }
        if state
            .sources
            .get(&SourceId("tool-schemas"))
            .is_some_and(SourceEpoch::is_changed)
        {
            report.tool_schemas_changed = true;
        }
        if state.model != state.prev_model {
            report.model_changed = true;
        }
        if state.fast_mode {
            report.fast_mode_changed = true;
        }
        if state
            .sources
            .get(&SourceId("cache-control"))
            .is_some_and(SourceEpoch::is_changed)
        {
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

    /// Insert or update a [`SourceEpoch`] for the given source id.
    ///
    /// On first observation the epoch is created with the content as its
    /// baseline. On subsequent calls a [`SourceDelta::Changed`] delta is
    /// applied so that `is_changed()` reflects divergence from the baseline.
    fn upsert_source_epoch(
        sources: &mut HashMap<SourceId, SourceEpoch>,
        id: SourceId,
        content: SourceContent,
    ) {
        use std::collections::hash_map::Entry;
        match sources.entry(id) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().apply_delta(SourceDelta::Changed(content));
            }
            Entry::Vacant(entry) => {
                entry.insert(SourceEpoch::new(content));
            }
        }
    }
}
