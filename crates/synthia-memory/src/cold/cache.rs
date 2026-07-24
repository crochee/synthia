use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use synthia_core::Error;

use super::store::{MemoryHit, MemoryStore, SearchQuery};
use crate::types::ColdEntry;

pub struct CachedStore<S: MemoryStore> {
    inner: S,
    cache: Arc<RwLock<Cache>>,
    capacity: usize,
}

struct Cache {
    entries: HashMap<String, ColdEntry>,
    order: VecDeque<String>,
    positions: HashMap<String, usize>,
}

impl<S: MemoryStore> CachedStore<S> {
    pub fn new(inner: S, capacity: usize) -> Self {
        Self {
            inner,
            cache: Arc::new(RwLock::new(Cache {
                entries: HashMap::new(),
                order: VecDeque::new(),
                positions: HashMap::new(),
            })),
            capacity,
        }
    }

    fn get_cached(&self, id: &str) -> Option<ColdEntry> {
        let mut cache = self.cache.write().unwrap();
        let entry = cache.entries.get(id)?.clone();
        if let Some(pos) = cache.positions.get(id).copied() {
            cache.order.remove(pos);
        }
        cache.order.push_front(id.to_string());
        cache.positions.insert(id.to_string(), 0);
        Some(entry)
    }

    fn put_cached(&self, entry: &ColdEntry) {
        let mut cache = self.cache.write().unwrap();
        if let Some(pos) = cache.positions.get(&entry.id).copied() {
            cache.order.remove(pos);
        } else if cache.entries.len() >= self.capacity
            && let Some(lru_id) = cache.order.pop_back()
        {
            cache.entries.remove(&lru_id);
            cache.positions.remove(&lru_id);
        }
        cache.order.push_front(entry.id.clone());
        cache.positions.insert(entry.id.clone(), 0);
        cache.entries.insert(entry.id.clone(), entry.clone());
    }

    fn evict_cached(&self, id: &str) {
        let mut cache = self.cache.write().unwrap();
        if let Some(pos) = cache.positions.remove(id) {
            cache.order.remove(pos);
        }
        cache.entries.remove(id);
    }
}

#[async_trait]
impl<S: MemoryStore> MemoryStore for CachedStore<S> {
    async fn insert(&self, entry: &ColdEntry) -> Result<(), Error> {
        self.inner.insert(entry).await?;
        self.put_cached(entry);
        Ok(())
    }

    async fn insert_batch(&self, entries: &[&ColdEntry]) -> Result<(), Error> {
        self.inner.insert_batch(entries).await?;
        for entry in entries {
            self.put_cached(entry);
        }
        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<ColdEntry>, Error> {
        if let Some(entry) = self.get_cached(id) {
            return Ok(Some(entry));
        }
        if let Some(entry) = self.inner.get(id).await? {
            self.put_cached(&entry);
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }

    async fn search(
        &self,
        query: &SearchQuery,
    ) -> Result<Vec<MemoryHit>, Error> {
        self.inner.search(query).await
    }

    async fn delete(&self, id: &str) -> Result<(), Error> {
        self.inner.delete(id).await?;
        self.evict_cached(id);
        Ok(())
    }

    async fn delete_batch(&self, ids: &[&str]) -> Result<(), Error> {
        self.inner.delete_batch(ids).await?;
        for id in ids {
            self.evict_cached(id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::cold::in_memory::InMemoryStore;

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
    async fn test_cache_hit() {
        let inner = InMemoryStore::new();
        let cached = CachedStore::new(inner, 10);
        let entry = test_entry("1", "hello");
        cached.insert(&entry).await.unwrap();

        // First get - populates cache
        let result = cached.get("1").await.unwrap();
        assert!(result.is_some());

        // Second get - cache hit
        let result = cached.get("1").await.unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_lru_eviction() {
        let inner = InMemoryStore::new();
        let cached = CachedStore::new(inner, 2);
        cached.insert(&test_entry("1", "one")).await.unwrap();
        cached.insert(&test_entry("2", "two")).await.unwrap();
        cached.insert(&test_entry("3", "three")).await.unwrap();

        // "1" was evicted due to LRU, but is still in backing store
        // get() triggers cache miss, then re-caches the entry
        assert!(cached.get("1").await.unwrap().is_some());
        // Now "1" is re-cached
        assert!(cached.get_cached("1").is_some());
        // "2" and "3" should still be accessible
        assert!(cached.get("2").await.unwrap().is_some());
        assert!(cached.get("3").await.unwrap().is_some());
    }
}
