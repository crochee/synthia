//! The scored-result type returned by
//! [`super::index::BM25Index::search`].
//!
//! Kept as a separate file because the type is small (5
//! lines) but referenced from both [`super::index`] and
//! [`super::matcher`], so co-locating it with either
//! would create a one-way visibility asymmetry.

/// Scored skill result from a BM25 search.
///
/// `name` is the skill's [`SkillMetadata::name`] — used
/// to look the skill back up in the caller-supplied
/// `&[Skill]`. `bm25_score` is the raw score (NOT
/// adjusted for skill priority — that adjustment is
/// applied by [`super::matcher::BM25Matcher`] to produce
/// the final `final_score` on
/// [`crate::types::SkillMatch`]).
///
/// [`SkillMetadata::name`]: crate::types::SkillMetadata::name
#[derive(Clone, Debug)]
pub struct SkillScore {
    /// Skill name (matches `Skill::metadata::name`).
    pub name: String,
    /// Raw BM25 score, already filtered to `> 0.0`.
    pub bm25_score: f64,
}
