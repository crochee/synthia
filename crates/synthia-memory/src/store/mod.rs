//! Unified [`MemoryStoreImpl`] that combines all four
//! memory layers: hot / cold (JSONL + optional SQLite
//! for advanced retrieval) / episodic (JSONL) /
//! context.
//!
//! # Module Layout
//!
//! - [`builder`]: the [`builder::MemoryStoreImpl`]
//!   struct + 3 constructors (`new` / `with_paths` /
//!   `with_sqlite_cold`).
//! - [`hot`]: 2 methods on
//!   [`builder::MemoryStoreImpl`] ([`hot::MemoryStoreImpl::write_hot`]
//!   + [`hot::MemoryStoreImpl::read_hot`]).
//! - [`cold`]: 3 methods on
//!   [`builder::MemoryStoreImpl`]
//!   ([`cold::MemoryStoreImpl::append_cold`] +
//!   [`cold::MemoryStoreImpl::append_cold_fields`] +
//!   [`cold::MemoryStoreImpl::search_cold_jsonl`]).
//! - [`episodic`]: 3 methods on
//!   [`builder::MemoryStoreImpl`]
//!   ([`episodic::MemoryStoreImpl::write_episodic_jsonl`] +
//!   [`episodic::MemoryStoreImpl::write_episodic_fields`] +
//!   [`episodic::MemoryStoreImpl::load_episodic_jsonl`]).
//! - [`context`]: 3 methods on
//!   [`builder::MemoryStoreImpl`]
//!   ([`context::MemoryStoreImpl::set_context`] +
//!   [`context::MemoryStoreImpl::get_context`] +
//!   [`context::MemoryStoreImpl::compact_context`]).
//! - [`trait_impl`](trait_impl): the
//!   [`crate::types::MemoryStore`] trait impl for
//!   [`MemoryStoreImpl`], with 2 conversion helpers
//!   (`cold_jsonl_to_cold` + `episodic_jsonl_to_skill`).
//! - [`tests`]: 11 unit tests covering hot / cold /
//!   episodic / context / retrieval fallback.

mod builder;
mod cold;
mod context;
mod episodic;
mod hot;
mod trait_impl;

#[cfg(test)]
mod tests;

pub use builder::MemoryStoreImpl;
