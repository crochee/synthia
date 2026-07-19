//! Sink implementations behind [`EventSinkKind`]
//! ([`crate::EventSinkKind`]).
//!
//! PR-1.1 declares the module layout.
//! PR-1.3 lands the in-memory bounded ring.
//! PR-1.4 (gated by the `sqlite` feature) lands the durable dual-table sink.

pub mod in_memory;

#[cfg(feature = "sqlite")]
pub mod sqlite;
