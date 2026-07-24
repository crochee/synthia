//! Dense-vector skill matching on [`super::SkillRegistry`].
//!
//! [`SkillRegistry::match_skills_dense`] embeds the task
//! description with the configured embedding provider,
//! then uses [`crate::embedding::cosine_similarity_dense_search`]
//! to score every entry in the dense index. It returns
//! an empty vec if no provider is configured, if the
//! dense index is empty, or if the embed call fails.

use super::SkillRegistry;
use crate::types::{MatchStrategy, SkillMatch};

impl SkillRegistry {
    /// Match skills using dense vector embeddings. Returns empty vec if dense matching unavailable.
    pub(crate) async fn match_skills_dense(
        &self,
        task_description: &str,
        skills: &[crate::types::Skill],
    ) -> Vec<SkillMatch> {
        let provider = match &self.embedding_provider {
            Some(p) => p,
            None => return Vec::new(),
        };

        // Read the dense index and check if it has entries before making any async calls
        let (dense_index_snapshot, max_level0_inject) = {
            let dense_index = self.dense_index.read();
            if dense_index.is_empty() {
                return Vec::new();
            }
            // Clone the index data so we don't hold the lock across the await
            let vectors: Vec<_> = dense_index
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            (vectors, self.match_config.max_level0_inject)
        };

        // Embed the query (async call, no locks held)
        let embeddings = match provider
            .embed(vec![task_description.to_string()])
            .await
        {
            Ok(mut emb) => emb.pop(),
            Err(e) => {
                tracing::warn!(error = ?e, "Failed to embed query for dense matching");
                return Vec::new();
            }
        };

        let query_embedding = match embeddings {
            Some(e) => e,
            None => return Vec::new(),
        };

        // Search using the snapshot (no lock needed)
        let results = crate::embedding::cosine_similarity_dense_search(
            &query_embedding,
            &dense_index_snapshot,
            max_level0_inject,
        );

        // Convert to SkillMatch
        let skills_map: std::collections::HashMap<_, _> = skills
            .iter()
            .map(|s| (s.metadata.name.clone(), s.clone()))
            .collect();

        results
            .into_iter()
            .filter_map(|(name, score)| {
                let skill = skills_map.get(&name)?;
                Some(SkillMatch {
                    skill: skill.clone(),
                    final_score: score,
                    bm25_score: score,
                    matched_by: MatchStrategy::Vector,
                })
            })
            .collect()
    }
}
