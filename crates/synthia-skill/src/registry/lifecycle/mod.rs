//! Lifecycle: construction, loading, embedding,
//! dependency resolution, activation / deactivation, and
//! the various "lifecycle getter" helpers.
//!
//! This is the first of two `impl SkillRegistry` blocks
//! for the registry; the second lives in `super::query`.
//!
//! # Module Layout
//!
//! - [`constructors`]: 2 constructors
//!   ([`constructors::SkillRegistry::new`] +
//!   [`constructors::SkillRegistry::new_with_provider`]).
//! - [`loading`]: 4 loading + index-rebuild methods
//!   ([`loading::SkillRegistry::load_builtins`],
//!   [`loading::SkillRegistry::load_from_paths`],
//!   `rebuild_vector_index_internal`,
//!   `rebuild_bm25_index_internal`,
//!   `load_skill`).
//! - [`matching`]:
//!   [`matching::SkillRegistry::match_skills_dense`] (uses
//!   the embedding provider + dense_index snapshot).
//! - [`graph`]: 2 private graph helpers
//!   ([`graph::SkillRegistry::resolve_dependencies`] +
//!   [`graph::SkillRegistry::check_conflicts`]).
//! - [`activation`]: 2 activation methods
//!   ([`activation::SkillRegistry::activate_skill`] +
//!   [`activation::SkillRegistry::deactivate_skill`]).
//! - [`getters`]: 10 getter methods
//!   ([`getters::SkillRegistry::is_disabled`],
//!   `get_skill_sync`, `is_active`, `session_skill_tokens`,
//!   `active_skills`, `get_skill_map`,
//!   `get_level0_summaries`, `contains`, `len`, `is_empty`).

mod activation;
mod constructors;
mod getters;
mod graph;
mod loading;
mod matching;

use super::types::SkillRegistry;
