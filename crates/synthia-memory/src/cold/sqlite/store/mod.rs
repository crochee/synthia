//! `SqliteStore` — `MemoryStore` implementation over SQLite.
//!
//! A flat single-table implementation that satisfies the [`MemoryStore`]
//! trait used by the cache layer and `ColdStore` newtype. The schema is
//! the canonical `cold_entries` table with one row per `ColdEntry`.

mod memory_store;
mod r#struct;
#[cfg(test)]
mod tests;

pub use r#struct::SqliteStore;
