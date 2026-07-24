//! The high-level [`MatchingStrategy`] selector used by
//! callers that want to abstract over the concrete
//! [`super::bm25_matcher::BM25Matcher`],
//! [`super::keyword::KeywordMatcher`], and
//! [`super::hybrid::HybridMatcher`] strategies.
//!
//! The enum is intentionally separate from
//! [`crate::types::MatchStrategy`]: `MatchStrategy`
//! records **which** strategy produced a match (it lives
//! on every [`crate::types::SkillMatch`]); `MatchingStrategy`
//! is the **selector** — it is a config input the
//! caller passes in to choose which matcher to run.

/// Selector for which concrete matcher the registry
/// should run.
///
/// `Default` is [`MatchingStrategy::Keyword`]: when no
/// strategy is explicitly chosen, the cheapest
/// (substring) match wins. BM25 / hybrid are opt-in
/// because they carry a one-time index-build cost that
/// is only worth it for registries with many skills or
/// repeated queries.
#[derive(Debug, Clone, Default)]
pub enum MatchingStrategy {
    /// Substring match against each skill's trigger list
    /// (delegated to [`super::keyword::KeywordMatcher`]).
    #[default]
    Keyword,
    /// Vector embedding similarity. The `Embedding`
    /// variant is currently a placeholder — the real
    /// embedding path runs through
    /// [`super::hybrid::HybridMatcher`] with `bm25_weight
    /// = 0.0` once a [`crate::embedding::SparseVectorIndex`]
    /// is supplied.
    Embedding,
    /// Weighted blend of BM25 + vector similarity
    /// (delegated to [`super::hybrid::HybridMatcher`])
    /// with the supplied `keyword_weight` /
    /// `embedding_weight` blend ratios.
    Hybrid {
        /// Weight applied to the BM25 leg of the blend.
        keyword_weight: f64,
        /// Weight applied to the vector-similarity leg.
        embedding_weight: f64,
    },
}
