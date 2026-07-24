use std::{collections::HashMap, sync::RwLock};

use async_trait::async_trait;
use synthia_core::Error;

use super::store::{MemoryHit, MemoryStore, SearchQuery};
use crate::types::ColdEntry;

pub struct InMemoryStore {
    entries: RwLock<HashMap<String, ColdEntry>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MemoryStore for InMemoryStore {
    async fn insert(&self, entry: &ColdEntry) -> Result<(), Error> {
        let entry = entry.clone();
        self.entries
            .write()
            .unwrap()
            .insert(entry.id.clone(), entry);
        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<ColdEntry>, Error> {
        Ok(self.entries.read().unwrap().get(id).cloned())
    }

    async fn search(
        &self,
        query: &SearchQuery,
    ) -> Result<Vec<MemoryHit>, Error> {
        let query_lower = query.query.to_lowercase();
        let mut hits: Vec<_> = self
            .entries
            .read()
            .unwrap()
            .values()
            .filter(|e| e.content.to_lowercase().contains(&query_lower))
            .map(|e| MemoryHit {
                entry: e.clone(),
                score: 1.0,
            })
            .collect();
        hits.truncate(query.limit);
        Ok(hits)
    }

    async fn delete(&self, id: &str) -> Result<(), Error> {
        self.entries.write().unwrap().remove(id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn test_entry(id: &str, content: &str) -> ColdEntry {
        ColdEntry {
            id: id.to_string(),
            content: content.to_string(),
            metadata: serde_json::Value::Null,
            created_at: Utc::now(),
            timestamp: None,
            summary: None,
            session_id: None,
            tools_used: None,
            outcome: None,
            importance_score: 0.5,
            access_count: 0,
        }
    }

    #[tokio::test]
    async fn test_insert_and_get() {
        let store = InMemoryStore::new();
        let entry = test_entry("1", "hello world");
        store.insert(&entry).await.unwrap();
        let result = store.get("1").await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "1");
    }

    #[tokio::test]
    async fn test_get_not_found() {
        let store = InMemoryStore::new();
        let result = store.get("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete() {
        let store = InMemoryStore::new();
        let entry = test_entry("1", "hello");
        store.insert(&entry).await.unwrap();
        store.delete("1").await.unwrap();
        assert!(store.get("1").await.unwrap().is_none());
    }
}
