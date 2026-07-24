//! Mode dispatcher: maps a [`RetrievalMode`] to the
//! appropriate backend.

use sqlx::SqlitePool;
use synthia_core::Error;

use super::{bm25_search, hybrid_search, semantic_search};
use crate::types::{ColdEntry, RetrievalMode, SearchResult};

/// Execute retrieval with the specified mode.
pub async fn retrieve(
    pool: &SqlitePool,
    entries: &[ColdEntry],
    query: &str,
    limit: usize,
    mode: RetrievalMode,
) -> Result<Vec<SearchResult>, Error> {
    match mode {
        RetrievalMode::Bm25 => bm25_search(pool, query, limit).await,
        RetrievalMode::Semantic => Ok(semantic_search(entries, query, limit)),
        RetrievalMode::Hybrid => {
            hybrid_search(pool, entries, query, limit).await
        }
    }
}
