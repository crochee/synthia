use std::{collections::HashMap, path::PathBuf, sync::Arc};

use tokio::sync::RwLock;

use super::{
    constants::*,
    entry::MemoryEntry,
    format::{format_entry, key_to_filename, parse_entry},
};

pub struct HotMemory {
    store: Arc<RwLock<HashMap<String, MemoryEntry>>>,
    store_path: PathBuf,
    token_budget: usize,
}

impl Clone for HotMemory {
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
            store_path: self.store_path.clone(),
            token_budget: self.token_budget,
        }
    }
}

impl HotMemory {
    pub fn new(store_path: PathBuf) -> Self {
        Self::with_budget(store_path, 32_000)
    }

    pub fn with_budget(store_path: PathBuf, token_budget: usize) -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
            store_path,
            token_budget,
        }
    }

    /// Resolve the full file path for a given key.
    fn file_path(&self, key: &str) -> PathBuf {
        self.store_path.join(key_to_filename(key))
    }

    /// Load all hot memory files from disk into the in-memory store.
    pub async fn load_from_disk(&self) -> Result<(), synthia_core::Error> {
        let memory_path = self.file_path(MEMORY_MD_KEY);
        if memory_path.exists() {
            let content = tokio::fs::read_to_string(&memory_path).await?;
            if let Some(parsed) = parse_entry(&content) {
                let mut store = self.store.write().await;
                store.insert(
                    MEMORY_MD_KEY.to_string(),
                    MemoryEntry::new(parsed, false),
                );
            }
        }

        let user_path = self.file_path(USER_MD_KEY);
        if user_path.exists() {
            let content = tokio::fs::read_to_string(&user_path).await?;
            if let Some(parsed) = parse_entry(&content) {
                let mut store = self.store.write().await;
                store.insert(
                    USER_MD_KEY.to_string(),
                    MemoryEntry::new(parsed, false),
                );
            }
        }

        Ok(())
    }

    /// Write a hot memory entry as markdown with frontmatter.
    pub async fn write(
        &self,
        key: &str,
        content: &str,
    ) -> Result<(), synthia_core::Error> {
        let formatted = format_entry(key, content);

        {
            let mut store = self.store.write().await;
            store.insert(
                key.to_string(),
                MemoryEntry::new(content.to_string(), true),
            );
            self.evict_over_budget(&mut store).await;
        }

        let path = self.file_path(key);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, formatted).await?;

        Ok(())
    }

    /// Read a hot memory entry, returning content without frontmatter.
    pub async fn read(
        &self,
        key: &str,
    ) -> Result<Option<String>, synthia_core::Error> {
        {
            let mut store = self.store.write().await;
            if let Some(entry) = store.get_mut(key) {
                entry.last_accessed = std::time::Instant::now();
                return Ok(Some(entry.value.clone()));
            }
        }

        let path = self.file_path(key);
        if path.exists() {
            let raw = tokio::fs::read_to_string(&path).await?;
            let parsed = parse_entry(&raw).unwrap_or_else(|| raw.clone());
            {
                let mut store = self.store.write().await;
                store.insert(
                    key.to_string(),
                    MemoryEntry::new(parsed.clone(), false),
                );
            }
            return Ok(Some(parsed));
        }

        Ok(None)
    }

    pub async fn read_all(
        &self,
    ) -> Result<HashMap<String, String>, synthia_core::Error> {
        let store = self.store.read().await;
        Ok(store
            .iter()
            .map(|(k, v)| (k.clone(), v.value.clone()))
            .collect())
    }

    pub async fn total_token_estimate(&self) -> usize {
        let store = self.store.read().await;
        store.values().map(|e| e.token_estimate()).sum()
    }

    async fn evict_over_budget(
        &self,
        store: &mut HashMap<String, MemoryEntry>,
    ) {
        let mut total: usize = store.values().map(|e| e.token_estimate()).sum();

        while total > self.token_budget && store.len() > 2 {
            let oldest_key = store
                .iter()
                .filter(|(k, _)| *k != MEMORY_MD_KEY && *k != USER_MD_KEY)
                .min_by_key(|(_, e)| e.last_accessed)
                .map(|(k, _)| k.clone());

            if let Some(key) = oldest_key {
                if let Some(evicted) = store.remove(&key) {
                    total -= evicted.token_estimate();
                    tracing::debug!(key = %key, tokens_evicted = evicted.token_estimate(), "HotMemory: evicted entry over token budget");
                }
            } else {
                break;
            }
        }
    }

    /// Read MEMORY.md content specifically.
    pub async fn read_memory(
        &self,
    ) -> Result<Option<String>, synthia_core::Error> {
        self.read(MEMORY_MD_KEY).await
    }

    /// Read USER.md content specifically.
    pub async fn read_user(
        &self,
    ) -> Result<Option<String>, synthia_core::Error> {
        self.read(USER_MD_KEY).await
    }

    /// Returns all dirty entries (key, value pairs) that need to be flushed.
    pub async fn get_dirty_entries(&self) -> HashMap<String, String> {
        let store = self.store.read().await;
        store
            .iter()
            .filter(|(_, v)| v.dirty)
            .map(|(k, v)| (k.clone(), v.value.clone()))
            .collect()
    }

    /// Marks all entries as clean (not dirty). Called after successful flush.
    pub async fn clear_dirty_flags(&self) {
        let mut store = self.store.write().await;
        for entry in store.values_mut() {
            entry.dirty = false;
        }
    }

    /// Returns true if any entries are dirty.
    pub async fn has_dirty_entries(&self) -> bool {
        let store = self.store.read().await;
        store.values().any(|v| v.dirty)
    }

    /// Flush all dirty entries to disk. Called by the persistence layer.
    pub async fn flush_dirty(&self) -> Result<(), synthia_core::Error> {
        let dirty = self.get_dirty_entries().await;
        if dirty.is_empty() {
            return Ok(());
        }

        for (key, value) in &dirty {
            let formatted = format_entry(key, value);
            let path = self.file_path(key);
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(&path, formatted).await?;
        }

        self.clear_dirty_flags().await;
        tracing::debug!(
            entries_flushed = dirty.len(),
            "HotMemory: flushed dirty entries to disk"
        );

        Ok(())
    }
}
