use async_trait::async_trait;

pub mod cache;
pub mod in_memory;
pub mod sqlite;
pub mod store;

pub use cache::CachedStore;
pub use sqlite::{ColdMemory, SqliteStore};
pub use store::{MemoryHit, MemoryStore, SearchMode, SearchQuery};

/// ColdStore = newtype around CachedStore<SqliteStore> for production use.
/// Cache-first cold storage with SQLite persistence.
pub struct ColdStore(CachedStore<SqliteStore>);

impl ColdStore {
    /// Create a new ColdStore with file-backed SQLite and given cache capacity.
    pub async fn new(
        database_path: &std::path::Path,
        cache_capacity: usize,
    ) -> Result<Self, synthia_core::Error> {
        let sqlite =
            SqliteStore::new(&format!("sqlite:{}", database_path.display()))
                .await?;
        Ok(Self(CachedStore::new(sqlite, cache_capacity)))
    }

    /// Create a new ColdStore with an in-memory SQLite database.
    pub async fn new_in_memory(
        cache_capacity: usize,
    ) -> Result<Self, synthia_core::Error> {
        let sqlite = SqliteStore::new("sqlite::memory:").await?;
        Ok(Self(CachedStore::new(sqlite, cache_capacity)))
    }

    /// Wrapper around CachedStore for MemoryStore trait access.
    pub fn inner(&self) -> &CachedStore<SqliteStore> {
        &self.0
    }
}

// Delegate MemoryStore trait to inner CachedStore
#[async_trait]
impl MemoryStore for ColdStore {
    async fn insert(
        &self,
        entry: &crate::types::ColdEntry,
    ) -> Result<(), synthia_core::Error> {
        self.0.insert(entry).await
    }

    async fn insert_batch(
        &self,
        entries: &[&crate::types::ColdEntry],
    ) -> Result<(), synthia_core::Error> {
        self.0.insert_batch(entries).await
    }

    async fn get(
        &self,
        id: &str,
    ) -> Result<Option<crate::types::ColdEntry>, synthia_core::Error> {
        self.0.get(id).await
    }

    async fn search(
        &self,
        query: &SearchQuery,
    ) -> Result<Vec<MemoryHit>, synthia_core::Error> {
        self.0.search(query).await
    }

    async fn delete(&self, id: &str) -> Result<(), synthia_core::Error> {
        self.0.delete(id).await
    }

    async fn delete_batch(
        &self,
        ids: &[&str],
    ) -> Result<(), synthia_core::Error> {
        self.0.delete_batch(ids).await
    }
}
