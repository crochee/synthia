use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use synthia_core::Error;
use synthia_memory::types::{
    ColdEntry,
    CompactionReport,
    EpisodicSkill,
    RetrievalMode,
    SearchResult,
};
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct FakeMemoryStore {
    hot: Arc<RwLock<HashMap<String, String>>>,
    cold: Arc<RwLock<Vec<ColdEntry>>>,
    episodic: Arc<RwLock<Vec<EpisodicSkill>>>,
}

impl FakeMemoryStore {
    pub fn new() -> Self {
        Self {
            hot: Arc::new(RwLock::new(HashMap::new())),
            cold: Arc::new(RwLock::new(Vec::new())),
            episodic: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl Default for FakeMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl synthia_memory::types::MemoryStore for FakeMemoryStore {
    async fn write_hot(&self, key: &str, value: &str) -> Result<(), Error> {
        self.hot
            .write()
            .await
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    async fn read_hot(&self, key: &str) -> Result<Option<String>, Error> {
        Ok(self.hot.read().await.get(key).cloned())
    }

    async fn search_cold(
        &self,
        _query: &str,
        limit: usize,
    ) -> Result<Vec<ColdEntry>, Error> {
        let entries = self.cold.read().await;
        Ok(entries.iter().take(limit).cloned().collect())
    }

    async fn search_cold_with_mode(
        &self,
        _query: &str,
        limit: usize,
        _mode: RetrievalMode,
    ) -> Result<Vec<SearchResult>, Error> {
        let entries = self.cold.read().await;
        Ok(entries
            .iter()
            .take(limit)
            .map(|e| SearchResult {
                entry: e.clone(),
                score: 0.5,
            })
            .collect())
    }

    async fn write_cold(&self, entry: ColdEntry) -> Result<(), Error> {
        self.cold.write().await.push(entry);
        Ok(())
    }

    async fn load_episodic(
        &self,
        task_hint: &str,
    ) -> Result<Vec<EpisodicSkill>, Error> {
        let skills = self.episodic.read().await;
        Ok(skills
            .iter()
            .filter(|s| s.task_hint.contains(task_hint))
            .cloned()
            .collect())
    }

    async fn save_episodic(&self, skill: EpisodicSkill) -> Result<(), Error> {
        self.episodic.write().await.push(skill);
        Ok(())
    }

    async fn compact_context(
        &self,
        _session_id: &str,
    ) -> Result<CompactionReport, Error> {
        Ok(CompactionReport::default())
    }
}

// Standalone tests module
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_write_and_read_hot() {
        let store = FakeMemoryStore::new();
        // Use the trait method directly
        synthia_memory::types::MemoryStore::write_hot(&store, "key1", "value1")
            .await
            .unwrap();
        assert_eq!(
            synthia_memory::types::MemoryStore::read_hot(&store, "key1")
                .await
                .unwrap(),
            Some("value1".to_string())
        );
    }

    #[tokio::test]
    async fn test_read_nonexistent_hot() {
        let store = FakeMemoryStore::new();
        assert_eq!(
            synthia_memory::types::MemoryStore::read_hot(&store, "missing")
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn test_write_and_search_cold() {
        let store = FakeMemoryStore::new();
        let entry = ColdEntry {
            id: "1".to_string(),
            content: "test content".to_string(),
            metadata: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            ..Default::default()
        };
        synthia_memory::types::MemoryStore::write_cold(&store, entry)
            .await
            .unwrap();
        let results = synthia_memory::types::MemoryStore::search_cold(
            &store, "query", 10,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_search_cold_empty() {
        let store = FakeMemoryStore::new();
        let results = synthia_memory::types::MemoryStore::search_cold(
            &store, "query", 10,
        )
        .await
        .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_episodic_roundtrip() {
        let store = FakeMemoryStore::new();
        let skill = EpisodicSkill {
            task_hint: "summarize".to_string(),
            skill_content: "content".to_string(),
            success_rate: 0.9,
            used_at: chrono::Utc::now(),
        };
        synthia_memory::types::MemoryStore::save_episodic(&store, skill)
            .await
            .unwrap();
        let loaded = synthia_memory::types::MemoryStore::load_episodic(
            &store,
            "summarize",
        )
        .await
        .unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].task_hint, "summarize");
    }

    #[tokio::test]
    async fn test_compact_context_returns_default() {
        let store = FakeMemoryStore::new();
        let report =
            synthia_memory::types::MemoryStore::compact_context(&store, "s1")
                .await
                .unwrap();
        assert_eq!(report.tokens_before, 0);
        assert_eq!(report.tokens_after, 0);
    }
}
