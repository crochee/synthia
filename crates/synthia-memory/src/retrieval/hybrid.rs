//! Hybrid retrieval: combines BM25 and semantic scores.
//!
//! Algorithm:
//! 1. Run both [`super::bm25::bm25_search`] and
//!    [`super::semantic::semantic_search`].
//! 2. Normalize each score to [0, 1] by dividing by the
//!    source's max score.
//! 3. Take the union of all entry IDs (preserving the
//!    `ColdEntry` from whichever source had it).
//! 4. Combine:
//!    `weighted_score = bm25_weight * normalized_bm25 + semantic_weight * normalized_semantic`.
//! 5. Sort by combined score descending, truncate to `limit`.

use std::collections::{HashMap, HashSet};

use sqlx::SqlitePool;
use synthia_core::Error;

use super::{bm25::bm25_search, semantic::semantic_search};
use crate::types::{ColdEntry, SearchResult};

/// Hybrid retrieval: combines BM25 and semantic scores
/// with default weights 0.7 for BM25 and 0.3 for semantic.
pub async fn hybrid_search(
    pool: &SqlitePool,
    entries: &[ColdEntry],
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>, Error> {
    hybrid_search_with_weights(pool, entries, query, limit, 0.7, 0.3).await
}

/// Hybrid retrieval with configurable weights.
/// weighted_score = bm25_weight * normalized_bm25 + semantic_weight * normalized_semantic
pub async fn hybrid_search_with_weights(
    pool: &SqlitePool,
    entries: &[ColdEntry],
    query: &str,
    limit: usize,
    bm25_weight: f64,
    semantic_weight: f64,
) -> Result<Vec<SearchResult>, Error> {
    let bm25_results = bm25_search(pool, query, limit).await?;
    let semantic_results = semantic_search(entries, query, limit);

    // Collect all unique entry IDs
    let all_ids: HashSet<String> = bm25_results
        .iter()
        .map(|r| r.entry.id.clone())
        .chain(semantic_results.iter().map(|r| r.entry.id.clone()))
        .collect();

    // Normalize scores to 0-1 range
    let bm25_max = bm25_results.iter().map(|r| r.score).fold(0.0f64, f64::max);
    let semantic_max = semantic_results
        .iter()
        .map(|r| r.score)
        .fold(0.0f64, f64::max);

    let bm25_scores: HashMap<String, f64> = bm25_results
        .iter()
        .map(|r| {
            let normalized = if bm25_max > 0.0 {
                r.score / bm25_max
            } else {
                0.0
            };
            (r.entry.id.clone(), normalized)
        })
        .collect();

    let semantic_scores: HashMap<String, f64> = semantic_results
        .iter()
        .map(|r| {
            let normalized = if semantic_max > 0.0 {
                r.score / semantic_max
            } else {
                0.0
            };
            (r.entry.id.clone(), normalized)
        })
        .collect();

    let mut combined: Vec<SearchResult> = all_ids
        .iter()
        .filter_map(|id| {
            let bm25_score = bm25_scores.get(id).copied().unwrap_or(0.0);
            let semantic_score =
                semantic_scores.get(id).copied().unwrap_or(0.0);
            let combined_score =
                bm25_weight * bm25_score + semantic_weight * semantic_score;

            // Find the entry from either source
            let entry = bm25_results
                .iter()
                .find(|r| &r.entry.id == id)
                .or_else(|| semantic_results.iter().find(|r| &r.entry.id == id))
                .map(|r| r.entry.clone())?;

            Some(SearchResult {
                entry,
                score: combined_score,
            })
        })
        .collect();

    combined.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    combined.truncate(limit);

    Ok(combined)
}
