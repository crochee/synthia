//! SQLite-backed cold memory module.
//!
//! Submodule layout:
//!
//! - [`cold_memory`]: the `ColdMemory` struct itself — FTS5 + metadata
//!   tables, append/search/eviction semantics, BM25/semantic/hybrid
//!   retrieval, and markdown flush.
//! - [`store`]: the `SqliteStore` struct — a flat single-table
//!   implementation of the `MemoryStore` trait used by the cache layer
//!   and `ColdStore` newtype.

mod cold_memory;
mod store;

pub use cold_memory::ColdMemory;
pub use store::SqliteStore;
