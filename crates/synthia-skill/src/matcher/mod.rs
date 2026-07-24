//! Skill matching strategies.
//!
//! Four focused submodules expose the matcher family:
//!
//! - [`strategy`]: the [`strategy::MatchingStrategy`]
//!   selector enum callers pass in to choose which
//!   matcher to run. It is intentionally separate from
//!   [`crate::types::MatchStrategy`], which records
//!   *which* strategy produced a match.
//! - [`bm25_matcher`]: a **stateful** BM25 wrapper that
//!   owns an `Option<BM25Index>` and is used by
//!   [`hybrid`]. (Distinct from
//!   [`crate::bm25::BM25Matcher`], which is a stateless
//!   facade.)
//! - [`keyword`]: the cheapest matcher — substring match
//!   against each skill's trigger list. This is the
//!   `Default` of [`strategy::MatchingStrategy`].
//! - [`hybrid`]: a weighted blend of BM25 and
//!   [`crate::embedding::SparseVectorIndex`] vector
//!   similarity, sorted by combined score.
//!
//! The 14 unit tests for all four submodules live in
//! [`tests`].

mod bm25_matcher;
mod hybrid;
mod keyword;
mod strategy;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use bm25_matcher::BM25Matcher;
pub use hybrid::HybridMatcher;
pub use keyword::KeywordMatcher;
pub use strategy::MatchingStrategy;
