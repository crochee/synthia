//! `ColdMemory` — SQLite-backed cold memory with FTS5 + importance scoring.
//!
//! Provides append/search/delete operations over a SQLite database split
//! into a `cold_entries_fts` FTS5 virtual table (for BM25/semantic search)
//! and a `cold_entries_meta` table (for importance/access metadata).
//!
//! # Module Layout
//!
//! - [`core`]: The [`core::ColdMemory`] struct itself, plus its
//!   4 constructors / builders ([`core::ColdMemory::new`],
//!   [`core::ColdMemory::new_in_memory`],
//!   [`core::ColdMemory::with_max_entries`],
//!   [`core::ColdMemory::with_importance_decay_factor`]).
//! - [`schema`]: The [`schema::init_schema`] helper — FTS5 virtual
//!   table + metadata table creation. Called by both `new` and
//!   `new_in_memory`.
//! - [`mutate`]: All write-path operations
//!   ([`mutate::append`],
//!   [`mutate::increment_access`],
//!   [`mutate::decay_importance_scores`],
//!   [`mutate::evict_low_importance_entries`]).
//! - [`search`]: All read-path operations — the
//!   [`search::search_with_mode`] dispatcher plus 3 backends
//!   ([`search::bm25_search_joined`],
//!   [`search::semantic_search_sql`],
//!   [`search::hybrid_search_sql`]).
//! - [`admin`]: Maintenance operations
//!   ([`admin::load_all_entries`],
//!   [`admin::entry_count`],
//!   [`admin::delete_entries`],
//!   [`admin::flush_to_file`]).

mod admin;
#[cfg(test)]
mod admin_test;
mod core;
mod mutate;
mod schema;
mod search;

pub use core::ColdMemory;
