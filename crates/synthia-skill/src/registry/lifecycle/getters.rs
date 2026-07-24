//! 10 getter methods on [`super::SkillRegistry`].
//!
//! These are all simple lock reads (no I/O, no async).
//! - [`SkillRegistry::is_disabled`] — check the state
//!   store's disabled set.
//! - [`SkillRegistry::get_skill_sync`] — clone the skill
//!   out of the skills map (synchronous version of
//!   `get_skill` for use inside `Tool::call`).
//! - [`SkillRegistry::is_active`] — check the active set.
//! - [`SkillRegistry::session_skill_tokens`] — return the
//!   current session-level token counter (clamped >= 0).
//! - [`SkillRegistry::active_skills`] — snapshot the
//!   active set as a `Vec<String>`.
//! - [`SkillRegistry::get_skill_map`] — return a
//!   `RwLockReadGuard` over the full skills map (for
//!   diagnostics).
//! - [`SkillRegistry::get_level0_summaries`] — render a
//!   list of `- name: description` strings from a
//!   `&[SkillMatch]`.
//! - [`SkillRegistry::contains`] — `IndexMap::contains_key`
//!   over the skills map.
//! - [`SkillRegistry::len`] / `is_empty` — `IndexMap::len`
//!   / `is_empty`.

use std::sync::atomic::Ordering;

use indexmap::IndexMap;
use parking_lot::RwLockReadGuard;

use super::SkillRegistry;
use crate::types::{Skill, SkillMatch};

impl SkillRegistry {
    pub fn is_disabled(&self, name: &str) -> bool {
        self.state_store.read().disabled_skills.contains(name)
    }

    /// Synchronous version of `get_skill` for use inside Tool::call.
    pub fn get_skill_sync(
        &self,
        name: &str,
    ) -> Result<Skill, synthia_core::Error> {
        self.skills.read().get(name).cloned().ok_or_else(|| {
            synthia_core::Error::NotFound(format!("skill not found: {}", name))
        })
    }

    /// Check if a skill is currently active (Level 1 or higher).
    pub fn is_active(&self, name: &str) -> bool {
        self.active_skills.read().contains(name)
    }

    pub fn session_skill_tokens(&self) -> usize {
        self.session_token_counter.load(Ordering::Relaxed).max(0) as usize
    }

    pub fn active_skills(&self) -> Vec<String> {
        self.active_skills.read().iter().cloned().collect()
    }

    /// Returns a reference to the skills map for diagnostic purposes.
    pub fn get_skill_map(
        &self,
    ) -> RwLockReadGuard<'_, IndexMap<String, Skill>> {
        self.skills.read()
    }

    pub fn get_level0_summaries(&self, matches: &[SkillMatch]) -> Vec<String> {
        matches
            .iter()
            .map(|m| {
                format!(
                    "- {}: {}",
                    m.skill.metadata.name, m.skill.metadata.description
                )
            })
            .collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.skills.read().contains_key(name)
    }

    pub fn len(&self) -> usize {
        self.skills.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.skills.read().is_empty()
    }
}
