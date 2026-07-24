use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use crate::types::SkillUsageRecord;

/// Tracks skill usage statistics including match counts, activation counts, and token costs.
#[derive(Clone)]
pub struct SkillUsageTracker {
    records: Arc<parking_lot::RwLock<HashMap<String, SkillUsageRecord>>>,
    flush_count: Arc<AtomicUsize>,
    flush_threshold: usize,
    storage_path: Option<PathBuf>,
}

impl SkillUsageTracker {
    pub fn new() -> Self {
        Self {
            records: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            flush_count: Arc::new(AtomicUsize::new(0)),
            flush_threshold: 10,
            storage_path: None,
        }
    }

    pub fn with_storage_path(mut self, path: PathBuf) -> Self {
        self.storage_path = Some(path);
        self
    }

    pub fn with_flush_threshold(mut self, threshold: usize) -> Self {
        self.flush_threshold = threshold;
        self
    }

    pub fn record_match(&self, skill_name: &str, token_cost: usize) {
        let mut records = self.records.write();
        let record =
            records.entry(skill_name.to_string()).or_insert_with(|| {
                SkillUsageRecord {
                    skill_name: skill_name.to_string(),
                    ..Default::default()
                }
            });
        record.match_count += 1;
        record.estimated_token_cost += token_cost;
        record.last_matched = Some(chrono::Utc::now());
    }

    pub fn record_activation(&self, skill_name: &str, token_cost: usize) {
        let mut records = self.records.write();
        let record =
            records.entry(skill_name.to_string()).or_insert_with(|| {
                SkillUsageRecord {
                    skill_name: skill_name.to_string(),
                    ..Default::default()
                }
            });
        record.activation_count += 1;
        record.estimated_token_cost += token_cost;
        record.last_activated = Some(chrono::Utc::now());
    }

    pub fn get_stats(&self, skill_name: &str) -> Option<SkillUsageRecord> {
        self.records.read().get(skill_name).cloned()
    }

    pub fn get_all_stats(&self) -> Vec<SkillUsageRecord> {
        self.records.read().values().cloned().collect()
    }

    pub fn should_flush(&self) -> bool {
        self.flush_count.load(Ordering::Relaxed) >= self.flush_threshold
    }

    pub fn record_operation(&self) {
        self.flush_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn flush(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref path) = self.storage_path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let records = self.records.read();
            let json = serde_json::to_string_pretty(&*records)?;
            std::fs::write(path, json)?;
        }
        self.flush_count.store(0, Ordering::Relaxed);
        Ok(())
    }

    pub fn load(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref path) = self.storage_path
            && path.exists()
        {
            let content = std::fs::read_to_string(path)?;
            let records: HashMap<String, SkillUsageRecord> =
                serde_json::from_str(&content)?;
            let mut current = self.records.write();
            current.extend(records);
        }
        Ok(())
    }
}

impl Default for SkillUsageTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Emits a tracing span for a skill matching operation.
pub fn emit_skill_match_span(
    skill_name: &str,
    strategy: &str,
    score: f64,
) -> tracing::Span {
    tracing::info_span!(
        "skill_match",
        skill_name = %skill_name,
        strategy = %strategy,
        score = score,
    )
}

/// Emits a tracing span for a skill loading/activation operation.
pub fn emit_skill_load_span(
    skill_name: &str,
    token_cost: usize,
) -> tracing::Span {
    tracing::info_span!(
        "skill_load",
        skill_name = %skill_name,
        token_cost = token_cost,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracker_record_match() {
        let tracker = SkillUsageTracker::new();
        tracker.record_match("test_skill", 100);
        let stats = tracker.get_stats("test_skill").unwrap();
        assert_eq!(stats.match_count, 1);
        assert_eq!(stats.estimated_token_cost, 100);
    }

    #[test]
    fn test_tracker_record_activation() {
        let tracker = SkillUsageTracker::new();
        tracker.record_activation("test_skill", 200);
        let stats = tracker.get_stats("test_skill").unwrap();
        assert_eq!(stats.activation_count, 1);
        assert_eq!(stats.estimated_token_cost, 200);
    }

    #[test]
    fn test_tracker_multiple_operations() {
        let tracker = SkillUsageTracker::new();
        tracker.record_match("skill_a", 100);
        tracker.record_match("skill_a", 50);
        tracker.record_activation("skill_a", 200);
        tracker.record_match("skill_b", 75);

        let stats_a = tracker.get_stats("skill_a").unwrap();
        assert_eq!(stats_a.match_count, 2);
        assert_eq!(stats_a.activation_count, 1);
        assert_eq!(stats_a.estimated_token_cost, 350);

        let all = tracker.get_all_stats();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_tracker_flush_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage_stats.json");

        let tracker = SkillUsageTracker::new().with_storage_path(path.clone());
        tracker.record_match("skill_a", 100);
        tracker.record_activation("skill_a", 200);

        tracker.flush().unwrap();
        assert!(path.exists());

        let tracker2 = SkillUsageTracker::new().with_storage_path(path);
        tracker2.load().unwrap();

        let stats = tracker2.get_stats("skill_a").unwrap();
        assert_eq!(stats.match_count, 1);
        assert_eq!(stats.activation_count, 1);
    }

    #[test]
    fn test_tracker_flush_threshold() {
        let tracker = SkillUsageTracker::new().with_flush_threshold(3);
        assert!(!tracker.should_flush());
        tracker.record_operation();
        tracker.record_operation();
        assert!(!tracker.should_flush());
        tracker.record_operation();
        assert!(tracker.should_flush());
    }

    #[test]
    fn test_tracker_get_stats_unknown() {
        let tracker = SkillUsageTracker::new();
        assert!(tracker.get_stats("nonexistent").is_none());
    }

    #[test]
    fn test_emit_skill_match_span() {
        let span = emit_skill_match_span("test_skill", "bm25", 0.85);
        let _ = format!("{:?}", span);
    }

    #[test]
    fn test_emit_skill_load_span() {
        let span = emit_skill_load_span("test_skill", 500);
        let _ = format!("{:?}", span);
    }
}
