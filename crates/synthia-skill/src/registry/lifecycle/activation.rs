//! Skill activation / deactivation on
//! [`super::SkillRegistry`].
//!
//! - [`SkillRegistry::activate_skill`] — resolves the
//!   dependency graph topologically, checks for conflicts
//!   on the target skill, then iterates the activation
//!   order. Each skill that's not already `Activated` is
//!   promoted to `Level1` + `Activated`, the level1 token
//!   count is added to the session counter, and the
//!   `active_skills` set is updated. Returns the activated
//!   target `Skill`.
//! - [`SkillRegistry::deactivate_skill`] — drops the skill
//!   back to `Level0` + `Loaded`, removes it from the
//!   active set, and decrements the session counter by the
//!   level1 token count.

use std::{collections::HashSet, sync::atomic::Ordering};

use synthia_core::Error;

use super::SkillRegistry;
use crate::types::{Skill, SkillLevel, SkillState};

impl SkillRegistry {
    pub fn activate_skill(&self, name: &str) -> Result<Skill, Error> {
        // Resolve dependencies in topological order
        let mut visited = HashSet::new();
        let mut path = vec![];
        let activation_order =
            self.resolve_dependencies(name, &mut visited, &mut path)?;

        // Check conflicts for the target skill before any activation
        self.check_conflicts(name)?;

        // Activate each skill in topological order (excluding already active ones)
        let mut target_skill: Option<Skill> = None;

        for skill_name in &activation_order {
            {
                let mut skills = self.skills.write();
                let skill = skills.get_mut(skill_name).ok_or_else(|| {
                    Error::NotFound(format!("skill not found: {}", skill_name))
                })?;

                // Skip if already active
                if skill.state == SkillState::Activated {
                    if skill_name == name {
                        target_skill = Some(skill.clone());
                    }
                    continue;
                }

                if skill.state == SkillState::Disabled {
                    return Err(Error::InvalidItem(format!(
                        "skill is disabled: {}",
                        skill_name
                    )));
                }

                skill.level = SkillLevel::Level1;
                skill.state = SkillState::Activated;
                let tokens = skill.token_count.level1;
                self.active_skills.write().insert(skill_name.to_string());
                self.session_token_counter
                    .fetch_add(tokens as isize, Ordering::Relaxed);
                tracing::info!(
                    skill = skill_name,
                    tokens = tokens,
                    "Skill activated"
                );

                if skill_name == name {
                    target_skill = Some(skill.clone());
                }
            }
        }

        target_skill.ok_or_else(|| {
            Error::NotFound(format!("skill not found: {}", name))
        })
    }

    pub fn deactivate_skill(&self, name: &str) -> Result<(), Error> {
        let mut skills = self.skills.write();
        let skill = skills.get_mut(name).ok_or_else(|| {
            Error::NotFound(format!("skill not found: {}", name))
        })?;
        let tokens = skill.token_count.level1;
        self.session_token_counter
            .fetch_sub(tokens as isize, Ordering::Relaxed);
        skill.level = SkillLevel::Level0;
        skill.state = SkillState::Loaded;
        self.active_skills.write().remove(name);
        tracing::info!(skill = name, "Skill deactivated");
        Ok(())
    }
}
