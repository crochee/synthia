use async_trait::async_trait;
use synthia_core::Error;

use crate::types::ColdEntry;

/// A pure interface for cold memory storage.
///
/// Implementors: SqliteStore (production), InMemoryStore (testing).
#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn insert(&self, entry: &ColdEntry) -> Result<(), Error>;
    async fn insert_batch(&self, entries: &[&ColdEntry]) -> Result<(), Error> {
        for entry in entries {
            self.insert(entry).await?;
        }
        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<ColdEntry>, Error>;
    async fn get_batch(
        &self,
        ids: &[&str],
    ) -> Result<Vec<Option<ColdEntry>>, Error> {
        let mut results = Vec::with_capacity(ids.len());
        for id in ids {
            results.push(self.get(id).await?);
        }
        Ok(results)
    }

    async fn search(
        &self,
        query: &SearchQuery,
    ) -> Result<Vec<MemoryHit>, Error>;
    async fn delete(&self, id: &str) -> Result<(), Error>;
    async fn delete_batch(&self, ids: &[&str]) -> Result<(), Error> {
        for id in ids {
            self.delete(id).await?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub query: String,
    pub limit: usize,
    pub mode: SearchMode,
}

#[derive(Debug, Clone)]
pub enum SearchMode {
    Similarity,
    BM25,
}

#[derive(Debug, Clone)]
pub struct MemoryHit {
    pub entry: ColdEntry,
    pub score: f64,
}
