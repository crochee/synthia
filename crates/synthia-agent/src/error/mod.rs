//! Error types for synthia-agent
//!
//! The original 703-line `mod.rs` was split into focused
//! submodules by responsibility:
//!
//! - `core`: the [`AgentError`] enum (21 variants) and
//!   the [`ProviderErrorContext`] struct.
//! - `context`: `ProviderErrorContext`'s `Display` /
//!   `std::error::Error` impls and the
//!   `From<ProviderErrorContext> for AgentError` bridge.
//! - `constructors`: the 15 `AgentError::foo(...)`
//!   constructor methods.
//! - `predicates`: the 4 `AgentError::is_*(...)`
//!   classification methods.
//! - `from`: the 6 `From<ForeignType> for AgentError`
//!   conversions.
//!
//! The 34 unit tests live in `tests`.

mod constructors;
mod context;
mod core;
mod from;
mod predicates;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use core::{AgentError, ProviderErrorContext};
