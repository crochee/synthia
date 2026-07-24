//! The cheapest matcher: substring match against each
//! skill's trigger list.
//!
//! [`KeywordMatcher`] is the `Default` value of
//! [`super::strategy::MatchingStrategy`] — it's the
//! match the registry uses when the caller didn't pick
//! a strategy explicitly. Cheap to run (no index build)
//! but prone to false positives (a query containing the
//! substring "test" matches every skill with "test" in
//! any trigger).
//!
//! Returned `SkillMatch`es carry `bm25_score = 1.0` /
//! `final_score = 1.0` and are tagged with
//! [`MatchStrategy::Keyword`]. Callers that need a real
//! ranking should reach for the BM25 or hybrid matcher
//! instead.

use crate::types::{MatchStrategy, Skill, SkillMatch};

/// Substring-based skill matcher. Zero-sized — all
/// methods are associated.
pub struct KeywordMatcher;

impl KeywordMatcher {
    /// Return every skill whose trigger list contains
    /// any case-insensitive substring of
    /// `task_description`.
    ///
    /// The match is *unscored* — every match has
    /// `bm25_score = final_score = 1.0`. The match order
    /// follows the input `&[Skill]` order (no sorting
    /// is applied).
    pub fn match_skills(
        skills: &[Skill],
        task_description: &str,
    ) -> Vec<SkillMatch> {
        let task_lower = task_description.to_lowercase();
        skills
            .iter()
            .filter_map(|s| {
                let matched = s
                    .metadata
                    .triggers
                    .iter()
                    .any(|t| task_lower.contains(&t.to_lowercase()));
                if matched {
                    Some(SkillMatch {
                        skill: s.clone(),
                        bm25_score: 1.0,
                        final_score: 1.0,
                        matched_by: MatchStrategy::Keyword,
                    })
                } else {
                    None
                }
            })
            .collect()
    }
}
