//! BM25 retrieval using SQLite FTS5 full-text search.

use sqlx::SqlitePool;
use synthia_core::Error;

use crate::types::{ColdEntry, SearchResult};

/// BM25 retrieval using SQLite FTS5 full-text search.
pub async fn bm25_search(
    pool: &SqlitePool,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>, Error> {
    let rows: Vec<(String, String, f64)> = sqlx::query_as(
        r#"
        SELECT entry_id, content, bm25(cold_entries_fts) as rank
        FROM cold_entries_fts
        WHERE cold_entries_fts MATCH ?
        ORDER BY rank ASC
        LIMIT ?
        "#,
    )
    .bind(format_fts_query(query))
    .bind(limit as i64)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        Error::Io(std::io::Error::other(format!("BM25 search failed: {}", e)))
    })?;

    let results: Vec<SearchResult> = rows
        .into_iter()
        .map(|(id, content, rank)| {
            let entry = ColdEntry {
                id,
                content,
                metadata: serde_json::Value::Null,
                created_at: chrono::Utc::now(),
                ..Default::default()
            };
            let score = (-rank).max(0.0);
            SearchResult { entry, score }
        })
        .collect();

    Ok(results)
}

/// Convert a user query into an FTS5-compatible query string.
/// FTS5 supports: term, "phrase", NEAR, AND, OR, NOT
pub fn format_fts_query(query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // Use individual terms with OR for broader matching
    let terms: Vec<String> = trimmed
        .split_whitespace()
        .map(|t| t.replace('"', "\"\""))
        .collect();
    terms.join(" OR ")
}
