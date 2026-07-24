//! The 2 async search methods on
//! [`super::core::MemoryRetriever`]:
//!
//! - [`MemoryRetriever::search`] — the default wrapper
//!   around `search_with_mode` that pins the mode to
//!   [`crate::types::RetrievalMode::Bm25`].
//! - [`MemoryRetriever::search_with_mode`] — the 4-layer
//!   search that iterates hot / cold / episodic (and
//!   optionally semantic), applies per-layer scoring
//!   via the [`super::scoring`] helpers, sorts by
//!   `relevance` descending, and truncates to `limit`.
//!
//! An empty query (`query.trim().is_empty()`) short-circuits
//! and returns `Vec::new()`.

use chrono::Utc;

use super::{
    core::MemoryRetriever,
    scoring::{
        apply_importance_and_recency_weight,
        apply_importance_and_recency_weight_hours,
        apply_recency_weight_hours,
        compute_text_relevance,
    },
    types::MemorySearchResult,
};
use crate::types::RetrievalMode;

impl MemoryRetriever {
    pub async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Vec<MemorySearchResult> {
        self.search_with_mode(query, limit, RetrievalMode::Bm25)
            .await
    }

    pub async fn search_with_mode(
        &self,
        query: &str,
        limit: usize,
        mode: RetrievalMode,
    ) -> Vec<MemorySearchResult> {
        if query.trim().is_empty() {
            return Vec::new();
        }

        let now = Utc::now();
        let mut results = Vec::new();

        if let Ok(entries) = self.hot.read_all().await {
            for value in entries.values() {
                let base_score = compute_text_relevance(query, value);
                if base_score > 0.0 {
                    let timestamp = now.timestamp() as u64;
                    let importance_score = 0.5; // Hot memory default importance
                    let final_score = apply_importance_and_recency_weight(
                        base_score,
                        importance_score,
                        0,
                        now,
                        self.importance_weight,
                        self.recency_weight,
                    );
                    results.push(MemorySearchResult {
                        content: value.clone(),
                        source: "hot",
                        relevance: final_score,
                        timestamp,
                    });
                }
            }
        }

        if let Ok(entries) = self.cold.search(query, limit).await {
            for entry in entries {
                let base_score = compute_text_relevance(query, &entry.content);
                if base_score > 0.0 {
                    let timestamp = entry.created_at.timestamp() as u64;
                    let hours_since =
                        (now - entry.created_at).num_seconds() as f64 / 3600.0;
                    let final_score = apply_importance_and_recency_weight_hours(
                        base_score,
                        entry.importance_score,
                        hours_since,
                        self.importance_weight,
                        self.recency_weight,
                    );
                    results.push(MemorySearchResult {
                        content: entry.content,
                        source: "cold",
                        relevance: final_score,
                        timestamp,
                    });
                }
            }
        }

        if let Ok(skills) = self.episodic.load_all(limit).await {
            for skill in skills {
                let searchable =
                    format!("{} {}", skill.task_hint, skill.skill_content);
                let base_score = compute_text_relevance(query, &searchable);
                if base_score > 0.0 {
                    let timestamp = skill.used_at.timestamp() as u64;
                    let hours_since =
                        (now - skill.used_at).num_seconds() as f64 / 3600.0;
                    let importance_score = skill.success_rate; // Use success rate as importance score
                    let final_score = apply_importance_and_recency_weight_hours(
                        base_score,
                        importance_score,
                        hours_since,
                        self.importance_weight,
                        self.recency_weight,
                    );
                    results.push(MemorySearchResult {
                        content: format!(
                            "{}: {}",
                            skill.task_hint, skill.skill_content
                        ),
                        source: "episodic",
                        relevance: final_score,
                        timestamp,
                    });
                }
            }
        }

        if matches!(mode, RetrievalMode::Semantic | RetrievalMode::Hybrid)
            && let Some(ref retriever) = self.semantic_retriever
            && let Ok(semantic_results) =
                retriever.semantic_search(query, limit).await
        {
            for (id, score) in semantic_results {
                let final_score = apply_recency_weight_hours(
                    score as f64,
                    0.0,
                    self.recency_weight,
                );
                results.push(MemorySearchResult {
                    content: id,
                    source: "semantic",
                    relevance: final_score,
                    timestamp: now.timestamp() as u64,
                });
            }
        }

        results.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results.truncate(limit);
        results
    }
}
