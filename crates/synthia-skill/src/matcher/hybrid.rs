//! Hybrid matcher: a weighted blend of the BM25 leg
//! (run via the local [`super::bm25_matcher::BM25Matcher`])
//! and a vector-similarity leg (run via
//! [`crate::embedding::SparseVectorIndex`]).
//!
//! The blend formula is:
//!
//! ```text
//! combined_score = bm25_weight * bm25_score
//!               + (1.0 - bm25_weight) * vector_score
//! ```
//!
//! The result list is sorted by `combined_score`
//! descending and truncated to `top_k`. The
//! `SkillMatch`es carry `matched_by =
//! MatchStrategy::Vector` — this is intentional: the
//! hybrid scorer is fundamentally a vector scorer with
//! a BM25 prior, not the other way around.

use std::collections::HashMap;

use super::bm25_matcher::BM25Matcher;
use crate::{
    bm25::BM25Index,
    embedding::SparseVectorIndex,
    types::{MatchStrategy, Skill, SkillMatch},
};

/// Hybrid matcher combining BM25 and vector similarity
/// scores. Zero-sized — `match_skills` is an associated
/// function.
pub struct HybridMatcher;

impl HybridMatcher {
    /// Run the BM25 + vector blend on `skills`.
    ///
    /// * `bm25_index`: a pre-built [`BM25Index`] to use
    ///   for the BM25 leg. Required — the hybrid matcher
    ///   does NOT lazily build its own index (the caller
    ///   is expected to have one already, since the
    ///   hybrid path is only chosen when the caller has
    ///   paid the index-build cost).
    /// * `vector_index`: the
    ///   [`SparseVectorIndex`] for the vector leg.
    ///   `top_k` is forwarded to
    ///   [`SparseVectorIndex::search`].
    /// * `bm25_weight`: the blend weight for the BM25
    ///   leg. `0.0` = pure vector, `1.0` = pure BM25.
    /// * `top_k`: max number of results to return.
    pub fn match_skills(
        skills: &[Skill],
        task_description: &str,
        bm25_index: &BM25Index,
        vector_index: &SparseVectorIndex,
        bm25_weight: f64,
        top_k: usize,
    ) -> Vec<SkillMatch> {
        if skills.is_empty() {
            return Vec::new();
        }

        let mut matcher = BM25Matcher::new();
        matcher.index = Some(bm25_index.clone());
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let bm25_results = runtime.block_on(async {
            matcher.match_skills(task_description, skills).await
        });
        let vector_results = vector_index.search(task_description, top_k);

        let mut skill_scores: HashMap<String, (f64, f64)> = HashMap::new();

        for m in &bm25_results {
            let name = &m.skill.metadata.name;
            let (bm25, _vec) = skill_scores.entry(name.clone()).or_default();
            *bm25 = m.bm25_score;
        }

        for (name, vec_score) in &vector_results {
            let (_bm25, vec) = skill_scores.entry(name.clone()).or_default();
            *vec = *vec_score;
        }

        let mut scored: Vec<(String, f64)> = skill_scores
            .into_iter()
            .map(|(name, (bm25, vec))| {
                let combined = bm25_weight * bm25 + (1.0 - bm25_weight) * vec;
                (name, combined)
            })
            .collect();

        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(top_k);

        scored
            .into_iter()
            .filter_map(|(name, score)| {
                let skill = skills.iter().find(|s| s.metadata.name == name)?;
                Some(SkillMatch {
                    skill: skill.clone(),
                    bm25_score: score,
                    final_score: score,
                    matched_by: MatchStrategy::Vector,
                })
            })
            .collect()
    }
}
