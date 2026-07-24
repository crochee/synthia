//! Models cache module
//!
//! Provides disk-persisted model cache with TTL staleness.

use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

/// Cached model list entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub name: String,
    pub version: String,
    pub cached_at: chrono::DateTime<chrono::Utc>,
}

/// Cached model list
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelList {
    pub models: Vec<ModelEntry>,
}

/// Cache manager with TTL staleness
#[derive(Clone)]
pub struct ModelsCacheManager {
    cache_path: PathBuf,
    ttl: Duration,
}

impl ModelsCacheManager {
    pub fn new(cache_path: PathBuf, ttl: Duration) -> Self {
        Self { cache_path, ttl }
    }

    /// Load cache if fresh (not stale)
    pub async fn load_fresh(
        &self,
    ) -> Result<Option<ModelList>, std::io::Error> {
        if !self.cache_path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&self.cache_path)?;
        let cache: ModelList = serde_json::from_str(&content).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;

        // Check if stale
        if let Some(first) = cache.models.first() {
            let age = chrono::Utc::now() - first.cached_at;
            if age
                > chrono::Duration::from_std(self.ttl)
                    .unwrap_or(chrono::TimeDelta::MAX)
            {
                return Ok(None); // Cache is stale
            }
        }

        Ok(Some(cache))
    }

    /// Persist cache to disk
    pub async fn persist_cache(
        &self,
        models: &ModelList,
    ) -> Result<(), std::io::Error> {
        let content = serde_json::to_string_pretty(models).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;

        if let Some(parent) = self.cache_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&self.cache_path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tempfile::tempdir;

    use super::*;

    fn make_entry(name: &str, version: &str) -> ModelEntry {
        ModelEntry {
            name: name.to_string(),
            version: version.to_string(),
            cached_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_model_entry_serde_roundtrip() {
        let entry = make_entry("claude-3", "1.0");
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: ModelEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "claude-3");
        assert_eq!(parsed.version, "1.0");
    }

    #[test]
    fn test_model_list_serde_roundtrip() {
        let list = ModelList {
            models: vec![
                make_entry("model-a", "2.0"),
                make_entry("model-b", "3.0"),
            ],
        };
        let json = serde_json::to_string(&list).unwrap();
        let parsed: ModelList = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.models.len(), 2);
        assert_eq!(parsed.models[0].name, "model-a");
    }

    #[test]
    fn test_model_list_default() {
        let list = ModelList::default();
        assert!(list.models.is_empty());
    }

    #[test]
    fn test_models_cache_manager_new() {
        let cache = ModelsCacheManager::new(
            PathBuf::from("/tmp/cache.json"),
            Duration::from_secs(300),
        );
        assert_eq!(cache.cache_path, PathBuf::from("/tmp/cache.json"));
    }

    #[tokio::test]
    async fn test_load_fresh_nonexistent() {
        let cache = ModelsCacheManager::new(
            PathBuf::from("/tmp/nonexistent_cache_12345.json"),
            Duration::from_secs(300),
        );
        let result = cache.load_fresh().await;
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_load_fresh_stale_cache() {
        let dir = tempdir().unwrap();
        let cache_path = dir.path().join("stale_cache.json");
        let cache_mgr = ModelsCacheManager::new(
            cache_path.clone(),
            Duration::from_secs(300),
        ); // 5 min TTL
        // Create a cache entry with an old timestamp
        let mut entry = make_entry("claude-3", "1.0");
        entry.cached_at = chrono::Utc::now() - chrono::Duration::hours(10);
        let list = ModelList {
            models: vec![entry],
        };
        cache_mgr.persist_cache(&list).await.unwrap();

        let result = cache_mgr.load_fresh().await;
        assert!(result.unwrap().is_none()); // Stale -> None
    }

    #[tokio::test]
    async fn test_load_fresh_valid_cache() {
        let dir = tempdir().unwrap();
        let cache_path = dir.path().join("fresh_cache.json");
        let cache_mgr = ModelsCacheManager::new(
            cache_path.clone(),
            Duration::from_secs(3600),
        ); // 1 hour TTL
        let list = ModelList {
            models: vec![make_entry("gpt-4", "1.0")],
        };
        cache_mgr.persist_cache(&list).await.unwrap();

        let result = cache_mgr.load_fresh().await;
        assert!(result.is_ok());
        let loaded = result.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().models.len(), 1);
    }

    #[tokio::test]
    async fn test_persist_cache_creates_parent_dirs() {
        let dir = tempdir().unwrap();
        let cache_path =
            dir.path().join("subdir").join("deeper").join("cache.json");
        let cache_mgr = ModelsCacheManager::new(
            cache_path.clone(),
            Duration::from_secs(300),
        );
        let list = ModelList {
            models: vec![make_entry("model", "1.0")],
        };
        cache_mgr.persist_cache(&list).await.unwrap();
        assert!(cache_path.exists());
    }

    #[tokio::test]
    async fn test_load_fresh_invalid_json() {
        let dir = tempdir().unwrap();
        let cache_path = dir.path().join("bad_cache.json");
        std::fs::write(&cache_path, "not valid json {{{").unwrap();

        let cache_mgr =
            ModelsCacheManager::new(cache_path, Duration::from_secs(300));
        let result = cache_mgr.load_fresh().await;
        assert!(result.is_err());
    }
}
