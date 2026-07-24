//! All read-path operations on [`super::core::ColdMemory`].

use chrono::Utc;

use super::core::ColdMemory;
use crate::{
    retrieval::{format_fts_query, hybrid_search, semantic_search},
    types::{ColdEntry, RetrievalMode, SearchResult},
};

impl ColdMemory {
    /// Search using the default [`RetrievalMode`] and return just the entries.
    pub async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ColdEntry>, synthia_core::Error> {
        let results = self
            .search_with_mode(query, limit, self.default_mode)
            .await?;
        Ok(results.into_iter().map(|r| r.entry).collect())
    }

    /// Search with explicit retrieval mode. Returns full [`SearchResult`]s
    /// (entry + score) so callers can re-rank or log.
    pub async fn search_with_mode(
        &self,
        query: &str,
        limit: usize,
        mode: RetrievalMode,
    ) -> Result<Vec<SearchResult>, synthia_core::Error> {
        match mode {
            RetrievalMode::Bm25 => self.bm25_search_joined(query, limit).await,
            RetrievalMode::Semantic => {
                self.semantic_search_sql(query, limit).await
            }
            RetrievalMode::Hybrid => self.hybrid_search_sql(query, limit).await,
        }
    }

    /// BM25 search that fetches metadata in a single joined SQL query.
    pub(super) async fn bm25_search_joined(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, synthia_core::Error> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let fts_query = format_fts_query(query);
        let rows: Vec<(String, String, String, f64)> = sqlx::query_as(
            r#"
            SELECT f.entry_id, f.content, m.metadata, bm25(f) as rank
            FROM cold_entries_fts f
            JOIN cold_entries_meta m ON m.entry_id = f.entry_id
            WHERE f cold_entries_fts MATCH ?
            ORDER BY rank ASC
            LIMIT ?
            "#,
        )
        .bind(&fts_query)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            synthia_core::Error::Io(std::io::Error::other(format!(
                "BM25 joined search failed: {}",
                e
            )))
        })?;

        let results: Vec<SearchResult> = rows
            .into_iter()
            .map(|(id, content, metadata, rank)| {
                let parsed_metadata: serde_json::Value =
                    serde_json::from_str(&metadata)
                        .unwrap_or(serde_json::Value::Null);
                let entry = ColdEntry {
                    id,
                    content,
                    metadata: parsed_metadata,
                    created_at: Utc::now(),
                    ..Default::default()
                };
                let score = (-rank).max(0.0);
                SearchResult { entry, score }
            })
            .collect();

        Ok(results)
    }

    /// Semantic search: load matching candidates via SQL LIKE, then score in memory.
    pub(super) async fn semantic_search_sql(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, synthia_core::Error> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let pattern = format!("%{}%", query.to_lowercase());
        let rows: Vec<(String, String, String, f64)> = sqlx::query_as(
            r#"
            SELECT f.entry_id, f.content, m.metadata, m.importance_score
            FROM cold_entries_fts f
            JOIN cold_entries_meta m ON m.entry_id = f.entry_id
            WHERE f.content LIKE ?
            ORDER BY m.importance_score DESC
            LIMIT ?
            "#,
        )
        .bind(&pattern)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            synthia_core::Error::Io(std::io::Error::other(format!(
                "Semantic search failed: {}",
                e
            )))
        })?;

        let entries: Vec<ColdEntry> = rows
            .into_iter()
            .map(|(id, content, metadata, _importance_score)| {
                let parsed_metadata: serde_json::Value =
                    serde_json::from_str(&metadata)
                        .unwrap_or(serde_json::Value::Null);
                ColdEntry {
                    id,
                    content,
                    metadata: parsed_metadata,
                    created_at: Utc::now(),
                    ..Default::default()
                }
            })
            .collect();

        Ok(semantic_search(&entries, query, limit)
            .into_iter()
            .map(|r| SearchResult {
                entry: r.entry,
                score: r.score,
            })
            .collect())
    }

    /// Hybrid search: FTS candidate selection in SQL, then hybrid scoring in memory.
    pub(super) async fn hybrid_search_sql(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, synthia_core::Error> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let fts_query = format_fts_query(query);
        let rows: Vec<(String, String, String, f64)> = sqlx::query_as(
            r#"
            SELECT f.entry_id, f.content, m.metadata, bm25(f) as rank
            FROM cold_entries_fts f
            JOIN cold_entries_meta m ON m.entry_id = f.entry_id
            WHERE f cold_entries_fts MATCH ?
            ORDER BY rank ASC
            LIMIT ?
            "#,
        )
        .bind(&fts_query)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            synthia_core::Error::Io(std::io::Error::other(format!(
                "Hybrid FTS search failed: {}",
                e
            )))
        })?;

        let entries: Vec<ColdEntry> = rows
            .into_iter()
            .map(|(id, content, metadata, _rank)| {
                let parsed_metadata: serde_json::Value =
                    serde_json::from_str(&metadata)
                        .unwrap_or(serde_json::Value::Null);
                ColdEntry {
                    id,
                    content,
                    metadata: parsed_metadata,
                    created_at: Utc::now(),
                    ..Default::default()
                }
            })
            .collect();

        hybrid_search(&self.pool, &entries, query, limit).await
    }
}
