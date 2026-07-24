//! Memory retrieval — the 3 search backends (BM25 /
//! Semantic / Hybrid) and the mode dispatcher.
//!
//! # Module Layout
//!
//! - [`bm25`]: [`bm25::bm25_search`] (SQLite FTS5 full-text
//!   search via `bm25(cold_entries_fts)` rank) + the FTS5
//!   query formatter ([`bm25::format_fts_query`]).
//! - [`semantic`]: [`semantic::semantic_search`] (keyword
//!   matching with weighted scoring: exact word = 1.0,
//!   partial = 0.5, frequency bonus = +0.2 per extra
//!   occurrence, normalized to [0, 1]).
//! - [`hybrid`]: [`hybrid::hybrid_search`] (default
//!   weights 0.7 BM25 + 0.3 Semantic) +
//!   [`hybrid::hybrid_search_with_weights`] (configurable
//!   weights).
//! - [`dispatch`]: [`dispatch::retrieve`] (the mode
//!   dispatcher that maps [`crate::types::RetrievalMode`]
//!   to the 3 backends).
//! - [`tests`]: 10 unit tests.

mod bm25;
mod dispatch;
mod hybrid;
mod semantic;

#[cfg(test)]
mod tests;

pub use bm25::{bm25_search, format_fts_query};
pub use dispatch::retrieve;
pub use hybrid::{hybrid_search, hybrid_search_with_weights};
pub use semantic::semantic_search;
