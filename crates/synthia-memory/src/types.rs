use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Unified memory event enum for background task processing.
/// Defined in synthia-memory as the single source of truth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryEvent {
    SessionEnd {
        session_id: String,
        summary: String,
        tools_used: Vec<String>,
        outcome: String,
    },
    ToolExecuted {
        session_id: String,
        tool_name: String,
        success: bool,
    },
    MemoryFlush {
        key: String,
        content: String,
    },
}

impl MemoryEvent {
    pub fn session_end(
        session_id: String,
        summary: String,
        tools_used: Vec<String>,
        outcome: String,
    ) -> Self {
        Self::SessionEnd {
            session_id,
            summary,
            tools_used,
            outcome,
        }
    }

    pub fn tool_executed(
        session_id: String,
        tool_name: String,
        success: bool,
    ) -> Self {
        Self::ToolExecuted {
            session_id,
            tool_name,
            success,
        }
    }

    pub fn memory_flush(key: String, content: String) -> Self {
        Self::MemoryFlush { key, content }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotEntry {
    pub key: String,
    pub value: String,
    pub updated_at: DateTime<Utc>,
    pub importance_score: f64,
}

impl Default for HotEntry {
    fn default() -> Self {
        Self {
            key: String::new(),
            value: String::new(),
            updated_at: Utc::now(),
            importance_score: 0.5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ColdEntry {
    pub id: String,
    pub content: String,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub timestamp: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tools_used: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub outcome: Option<String>,
    pub importance_score: f64,
    pub access_count: u64,
}

impl ColdEntry {
    pub fn new_jsonl(
        id: String,
        timestamp: DateTime<Utc>,
        summary: String,
        session_id: String,
        tools_used: Vec<String>,
        outcome: String,
    ) -> Self {
        let content = format!("{} [outcome: {}]", summary, outcome);
        Self {
            id,
            content,
            metadata: serde_json::json!({}),
            created_at: timestamp,
            timestamp: Some(timestamp),
            summary: Some(summary),
            session_id: Some(session_id),
            tools_used: Some(tools_used),
            outcome: Some(outcome),
            importance_score: 0.5,
            access_count: 0,
        }
    }

    pub fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.timestamp.or(Some(self.created_at))
    }

    pub fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }

    pub fn update_importance(&mut self, decay_factor: f64) {
        self.importance_score *= decay_factor;
    }

    pub fn increment_access(&mut self) {
        self.access_count += 1;
        self.importance_score += 0.1 * (1.0 - self.importance_score);
    }
}

/// JSONL cold entry as specified: timestamp, summary, session_id, tools_used, outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColdJsonlEntry {
    pub timestamp: DateTime<Utc>,
    pub summary: String,
    pub session_id: String,
    pub tools_used: Vec<String>,
    pub outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicSkill {
    pub task_hint: String,
    pub skill_content: String,
    pub success_rate: f64,
    pub used_at: DateTime<Utc>,
}

/// JSONL episodic entry as specified: task_hint, tools_used, success_count, avg_tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicJsonlEntry {
    pub task_hint: String,
    pub tools_used: Vec<String>,
    pub success_count: u64,
    pub avg_tokens: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompactionReport {
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub stage: usize,
}

/// Result of a retrieval query with a relevance score for ranking.
///
/// Returned by `MemoryStore::search_cold_with_mode` so callers can
/// surface the relevance signal to LLM-prompt assembly, not just the
/// raw `ColdEntry` (which is what the unranked `search_cold` returns).
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub entry: ColdEntry,
    pub score: f64,
}

/// Retrieval mode for hybrid search.
///
/// `Hybrid` is the default: weighted combination of BM25 keyword
/// matching (SQLite FTS5) and the semantic placeholder that
/// currently falls back to keyword scoring. Kept as a `Copy` enum
/// so it can be passed by value through async signatures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RetrievalMode {
    /// BM25 keyword matching via SQLite FTS5.
    Bm25,
    /// Semantic similarity (placeholder: keyword-based weighted scoring).
    Semantic,
    /// Weighted combination of BM25 and semantic scores.
    #[default]
    Hybrid,
}

#[async_trait::async_trait]
pub trait MemoryStore: Send + Sync {
    async fn write_hot(
        &self,
        key: &str,
        value: &str,
    ) -> Result<(), synthia_core::Error>;
    async fn read_hot(
        &self,
        key: &str,
    ) -> Result<Option<String>, synthia_core::Error>;
    async fn search_cold(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ColdEntry>, synthia_core::Error>;
    async fn search_cold_with_mode(
        &self,
        query: &str,
        limit: usize,
        mode: RetrievalMode,
    ) -> Result<Vec<SearchResult>, synthia_core::Error>;
    async fn write_cold(
        &self,
        entry: ColdEntry,
    ) -> Result<(), synthia_core::Error>;
    async fn load_episodic(
        &self,
        task_hint: &str,
    ) -> Result<Vec<EpisodicSkill>, synthia_core::Error>;
    async fn save_episodic(
        &self,
        skill: EpisodicSkill,
    ) -> Result<(), synthia_core::Error>;
    async fn compact_context(
        &self,
        session_id: &str,
    ) -> Result<CompactionReport, synthia_core::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hot_entry_creation() {
        let entry = HotEntry {
            key: "user_prefs".to_string(),
            value: "verbose".to_string(),
            updated_at: Utc::now(),
            importance_score: 0.5,
        };
        assert_eq!(entry.key, "user_prefs");
    }

    #[test]
    fn test_cold_entry_creation() {
        let entry = ColdEntry {
            id: "1".to_string(),
            content: "test".to_string(),
            metadata: serde_json::json!({}),
            created_at: Utc::now(),
            ..Default::default()
        };
        assert_eq!(entry.content, "test");
    }

    #[test]
    fn test_compaction_report_default() {
        let report = CompactionReport::default();
        assert_eq!(report.tokens_before, 0);
        assert_eq!(report.stage, 0);
    }
}
