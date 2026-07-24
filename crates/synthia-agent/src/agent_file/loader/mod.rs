//! Agent-file loader family.
//!
//! The original 757-line `loader.rs` was split
//! into focused submodules by responsibility:
//!
//! - `types`: the [`AgentChangeEvent`]
//!   3-variant enum (re-exported at crate root).
//! - `loader`: the [`AgentFileLoader`] struct
//!   itself + its 8 methods
//!   ([`AgentFileLoader::new`],
//!   [`AgentFileLoader::list_ids`],
//!   [`AgentFileLoader::load`],
//!   [`AgentFileLoader::take_change_events`],
//!   [`AgentFileLoader::reload`],
//!   [`AgentFileLoader::detect_removals`],
//!   [`AgentFileLoader::watch`]).
//! - `extends`: the [`resolve_extends`] free
//!   function (the `extends` chain walker,
//!   re-exported at crate root).
//!
//! The 31 unit tests live in `tests`.

mod extends;
#[allow(clippy::module_inception)]
mod loader;
mod types;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use extends::resolve_extends;
pub use loader::AgentFileLoader;
pub use types::AgentChangeEvent;
