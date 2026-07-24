//! Multi-layer memory retriever — searches across hot,
//! cold, episodic, and (optionally) semantic stores with
//! recency + importance weighted ranking.
//!
//! # Module Layout
//!
//! - [`types`]: [`types::MemorySearchResult`] struct
//!   (content + source + relevance + timestamp).
//! - [`core`]: [`core::MemoryRetriever`] struct + 4
//!   constructors (`new` / `with_recency_weight` /
//!   `with_importance_weight` /
//!   `with_semantic_retriever`).
//! - [`search`]: 2 async search methods
//!   ([`search::MemoryRetriever::search`] +
//!   [`search::MemoryRetriever::search_with_mode`]) that
//!   iterate all 4 layers, apply per-layer scoring, sort
//!   by relevance descending, and truncate to `limit`.
//! - [`scoring`]: 4 private helper functions
//!   ([`scoring::compute_text_relevance`],
//!   `apply_recency_weight_hours`,
//!   `apply_importance_and_recency_weight`,
//!   `apply_importance_and_recency_weight_hours`).
//! - [`tests`]: 12 unit tests (6 text-relevance + 3
//!   recency-weight + 3 retriever integration).

mod core;
mod scoring;
mod search;
mod types;

#[cfg(test)]
mod tests;

pub use core::MemoryRetriever;

pub use types::MemorySearchResult;
