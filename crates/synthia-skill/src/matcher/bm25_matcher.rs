//! Stateful BM25 matcher used by [`super::hybrid::HybridMatcher`].
//!
//! ## A name-collision footnote
//!
//! This `BM25Matcher` is **distinct** from the
//! [`crate::bm25::BM25Matcher`]. The two coexist
//! intentionally:
//!
//! - [`crate::bm25::BM25Matcher`] is a **stateless
//!   facade** (zero-sized type) that takes a
//!   pre-built [`BM25Index`] and returns ranked
//!   `SkillMatch`es with the registry-facing
//!   priority-bonus adjustment. It is the entry point
//!   the skill registry calls.
//! - The `BM25Matcher` defined here is a **stateful
//!   wrapper** that owns an `Option<BM25Index>` — the
//!   index is built lazily on the first
//!   `match_skills` call when one wasn't pre-supplied.
//!   It exists so [`super::hybrid::HybridMatcher`] can
//!   share a single BM25 index across its BM25 leg and
//!   the vector leg, instead of rebuilding it on every
//!   query.
//!
//! A follow-up refactor may want to merge the two, but
//! doing so would change the call surface (the hybrid
//! matcher's ownership model depends on the stateful
//! variant). For now, the two coexist.
//!
//! The k1 / b parameters were dropped on 2026-06-18
//! because the actual BM25 scoring lives in
//! [`BM25Index::search`], which owns its own k1 / b.
//! This struct is now a thin query façade.

use crate::{
    bm25::BM25Index,
    types::{MatchStrategy, Skill, SkillMatch},
};

/// BM25-backed matcher. Wraps an optional pre-built
/// [`BM25Index`] (constructed lazily when `match_skills`
/// is called with an empty one).
pub struct BM25Matcher {
    /// The cached BM25 index. `None` means "build on
    /// demand from the next call's `&[Skill]`".
    pub(super) index: Option<BM25Index>,
}

impl BM25Matcher {
    /// Construct a matcher with no pre-built index —
    /// the next `match_skills` call will build it from
    /// the supplied skills.
    pub fn new() -> Self {
        Self { index: None }
    }

    /// Construct a matcher that reuses a pre-built
    /// index. Use this when the caller has already paid
    /// the index-build cost elsewhere.
    pub fn with_index(index: BM25Index) -> Self {
        Self { index: Some(index) }
    }

    /// Build (or rebuild) the cached index from a slice
    /// of skills. Subsequent `match_skills` calls will
    /// reuse the index until the next `build_index`
    /// call.
    pub fn build_index(&mut self, skills: &[Skill]) {
        self.index = Some(BM25Index::build(skills));
    }

    /// Search for skills matching `query`. Builds the
    /// index lazily on the first call if no pre-built
    /// index was supplied.
    ///
    /// The returned `SkillMatch`es carry the standard
    /// priority-bonus adjustment (`final_score =
    /// bm25_score * (1 + priority * 0.1)`) and are
    /// tagged with [`MatchStrategy::BM25`].
    pub async fn match_skills(
        &self,
        query: &str,
        skills: &[Skill],
    ) -> Vec<SkillMatch> {
        let scores = if let Some(ref index) = self.index {
            index.search(query)
        } else {
            let index = BM25Index::build(skills);
            index.search(query)
        };
        scores
            .into_iter()
            .filter(|s| s.bm25_score > 0.0)
            .filter_map(|scored| {
                skills.iter().find(|s| s.metadata.name == scored.name).map(
                    |skill| {
                        let priority_bonus =
                            1.0 + skill.metadata.priority as f64 * 0.1;
                        SkillMatch {
                            skill: skill.clone(),
                            final_score: scored.bm25_score * priority_bonus,
                            bm25_score: scored.bm25_score,
                            matched_by: MatchStrategy::BM25,
                        }
                    },
                )
            })
            .collect()
    }
}

impl Default for BM25Matcher {
    fn default() -> Self {
        Self::new()
    }
}
