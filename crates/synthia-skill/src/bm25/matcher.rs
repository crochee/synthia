//! The skill-registry-facing facade: take a raw
//! `&[Skill]` and a query, return ranked
//! [`SkillMatch`]es.
//!
//! [`BM25Matcher`] is a stateless unit struct — every
//! method is associated, no instance state. It owns the
//! **priority-bonus** adjustment: each matched skill's
//! raw BM25 score is multiplied by `1.0 + priority * 0.1`
//! to produce the `final_score` on the returned
//! `SkillMatch`. Source-based tiebreaking is NOT
//! applied here — the registry layer does that
//! downstream.
//!
//! [`SkillMatch`]: crate::types::SkillMatch

use super::index::BM25Index;
use crate::types::{MatchStrategy, Skill, SkillMatch};

/// BM25 matching strategy for the skill registry.
///
/// A zero-sized type — every method is `&self`-free
/// (they take the inputs explicitly so the registry
/// doesn't have to own a `BM25Matcher` instance).
pub struct BM25Matcher;

impl BM25Matcher {
    /// Match `skills` against `task_description` using
    /// the BM25 scores in `index`. Returns every skill
    /// with a non-zero raw score, sorted by `final_score`
    /// descending. See [`Self::match_skills_with_threshold`]
    /// for a variant that drops low scores.
    pub fn match_skills(
        skills: &[Skill],
        task_description: &str,
        index: &BM25Index,
    ) -> Vec<SkillMatch> {
        Self::match_skills_with_threshold(skills, task_description, index, 0.0)
    }

    /// Same as [`Self::match_skills`] but discards any
    /// `SkillMatch` whose raw BM25 score is below
    /// `min_score`. Use this to keep the result list
    /// focused on confident matches.
    pub fn match_skills_with_threshold(
        skills: &[Skill],
        task_description: &str,
        index: &BM25Index,
        min_score: f64,
    ) -> Vec<SkillMatch> {
        let scores = index.search(task_description);
        scores
            .into_iter()
            .filter(|s| s.bm25_score >= min_score)
            .filter_map(|scored| {
                let skill =
                    skills.iter().find(|s| s.metadata.name == scored.name)?;
                let priority_bonus = 1.0 + skill.metadata.priority as f64 * 0.1;
                Some(SkillMatch {
                    skill: skill.clone(),
                    bm25_score: scored.bm25_score,
                    final_score: scored.bm25_score * priority_bonus,
                    matched_by: MatchStrategy::BM25,
                })
            })
            .collect()
    }
}
