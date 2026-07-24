//! Skill registry — central in-memory index of every loaded skill.
//!
//! Submodule layout:
//!
//! - [`types`]: the [`SkillRegistry`] struct itself (shared state with
//!   `pub(super)` fields) plus the [`SkillFilter`] and the
//!   `RegistryItem` impl for `Skill`.
//! - [`lifecycle`]: the first of two `impl SkillRegistry` blocks —
//!   construction, loading, embedding, dependency resolution,
//!   activation/deactivation, and lifecycle getters.
//! - [`query`]: the second `impl SkillRegistry` block — list / get /
//!   match / enable-disable / reload / unregister.
//! - [`registry_trait`]: `impl Registry<Skill> for SkillRegistry`,
//!   isolated so the trait contract surface is discoverable in one place.
//!
//! Tests live in [`tests`].

mod lifecycle;
mod query;
mod registry_trait;
mod types;

#[cfg(test)]
mod tests;

pub use types::{SkillFilter, SkillRegistry};
