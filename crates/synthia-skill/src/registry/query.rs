//! Querying: list / get / match / enable-disable / reload / unregister.
//!
//! This is the second of two `impl SkillRegistry` blocks; the first
//! (lifecycle: construction, loading, embedding, activation) lives in
//! [`super::lifecycle`].

use std::path::Path;

use synthia_core::Error;

use super::types::SkillRegistry;
use crate::types::{SkillMatch, SkillMetadata, SkillState};

impl SkillRegistry {
    pub fn list_skills(&self) -> Vec<SkillMetadata> {
        self.skills
            .read()
            .iter()
            .map(|(_, s)| s.metadata.clone())
            .collect()
    }

    pub async fn get_skill(
        &self,
        name: &str,
    ) -> Result<crate::types::Skill, Error> {
        self.skills.read().get(name).cloned().ok_or_else(|| {
            Error::NotFound(format!("skill not found: {}", name))
        })
    }

    pub async fn match_skills(
        &self,
        task_description: &str,
    ) -> Vec<SkillMatch> {
        let skills: Vec<_> =
            self.skills.read().iter().map(|(_, v)| v.clone()).collect();
        let skill_count = skills.len();

        // Try dense vector matching first if provider is configured and index has entries
        let dense_results =
            self.match_skills_dense(task_description, &skills).await;
        let matches = if !dense_results.is_empty() {
            dense_results
        } else {
            // Fallback to TF-IDF / BM25 / keyword matching
            tracing::warn!(
                "Dense embedding index empty or unavailable, falling back to keyword/BM25 matching"
            );
            if skill_count < 20 {
                crate::matcher::KeywordMatcher::match_skills(
                    &skills,
                    task_description,
                )
            } else if skill_count <= 100 {
                let bm25_index = self.bm25_index.read();
                crate::bm25::BM25Matcher::match_skills(
                    &skills,
                    task_description,
                    &bm25_index,
                )
            } else {
                let bm25_index = self.bm25_index.read();
                let vector_index = self.vector_index.read();
                crate::matcher::HybridMatcher::match_skills(
                    &skills,
                    task_description,
                    &bm25_index,
                    &vector_index,
                    self.match_config.bm25_weight,
                    self.match_config.max_level0_inject,
                )
            }
        };

        let scored: Vec<_> = matches
            .into_iter()
            .map(|m| {
                let priority_bonus = 1.0
                    + m.skill.metadata.priority as f64
                        * self.match_config.priority_coefficient;
                SkillMatch {
                    final_score: m.bm25_score * priority_bonus,
                    ..m
                }
            })
            .collect();

        let mut sorted: Vec<_> = scored;
        sorted.sort_by(|a, b| {
            b.final_score
                .partial_cmp(&a.final_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.skill.source.cmp(&a.skill.source))
        });
        sorted.truncate(self.match_config.max_level0_inject);

        sorted.retain(|m| m.final_score >= self.match_config.min_match_score);
        sorted
    }

    pub fn register_from_path(&self, path: &Path) -> Result<(), Error> {
        self.load_skill(path)?;
        self.rebuild_bm25_index_internal();
        self.rebuild_vector_index_internal();
        Ok(())
    }

    pub fn disable(&self, name: &str) -> bool {
        if self.skills.read().contains_key(name) {
            self.state_store
                .write()
                .disabled_skills
                .insert(name.to_string());
            if let Some(skill) = self.skills.write().get_mut(name) {
                skill.state = SkillState::Disabled;
            }
            tracing::info!(skill = name, "Skill disabled");
            true
        } else {
            false
        }
    }

    pub fn enable(&self, name: &str) -> bool {
        self.state_store.write().disabled_skills.remove(name);
        if let Some(skill) = self.skills.write().get_mut(name)
            && skill.state == SkillState::Disabled
        {
            skill.state = SkillState::Loaded;
            tracing::info!(skill = name, "Skill enabled");
            return true;
        }
        false
    }

    pub fn reload(&self, path: &Path) -> Result<(), Error> {
        self.load_skill(path)?;
        self.rebuild_bm25_index_internal();
        self.rebuild_vector_index_internal();
        Ok(())
    }

    /// Synchronous skill removal.
    ///
    /// Inherent (non-async) wrapper used by sync call sites (CLI commands,
    /// file watcher callbacks, installer uninstall). The async
    /// `Registry::unregister` trait impl below delegates to this method.
    ///
    /// Returns `true` if the skill was present and removed, `false` if it
    /// was not in the registry. This preserves the bool semantic of the
    /// removed `SkillProvider::unregister` (deleted in change
    /// `2026-06-15-p1-skillprovider-remediation`).
    pub fn unregister(&self, name: &str) -> bool {
        let removed = self.skills.write().shift_remove(name).is_some();
        if removed {
            self.active_skills.write().remove(name);
            self.dense_index.write().remove(name);
            self.rebuild_bm25_index_internal();
            self.rebuild_vector_index_internal();
            tracing::info!(skill = name, "Skill unregistered");
        }
        removed
    }

    pub fn match_skills_vector(
        &self,
        task_description: &str,
        top_k: usize,
    ) -> Vec<(String, f64)> {
        self.vector_index.read().search(task_description, top_k)
    }

    pub fn rebuild_vector_index(&self) {
        self.rebuild_vector_index_internal();
    }
}
