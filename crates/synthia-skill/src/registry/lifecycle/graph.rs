//! Dependency-graph helpers on [`super::SkillRegistry`].
//!
//! - [`SkillRegistry::resolve_dependencies`] — recursive
//!   DFS over the `depends_on` field. Returns the
//!   topological activation order, or
//!   [`Error::InvalidItem`] if a cycle is detected.
//! - [`SkillRegistry::check_conflicts`] — returns
//!   [`Error::InvalidItem`] if any of the skill's
//!   `conflicts_with` is currently in the active set.

use std::collections::HashSet;

use synthia_core::Error;

use super::SkillRegistry;

impl SkillRegistry {
    pub(in crate::registry) fn resolve_dependencies(
        &self,
        name: &str,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Result<Vec<String>, Error> {
        if path.contains(&name.to_string()) {
            let cycle = path
                .iter()
                .skip(path.iter().position(|n| n == name).unwrap())
                .cloned()
                .collect::<Vec<_>>()
                .join(" -> ");
            return Err(Error::InvalidItem(format!(
                "circular dependency detected: {} -> {}",
                cycle, name
            )));
        }
        if visited.contains(name) {
            return Ok(vec![]); // Already resolved
        }

        let skills = self.skills.read();
        let skill = skills.get(name).ok_or_else(|| {
            Error::NotFound(format!("skill not found: {}", name))
        })?;

        let deps: Vec<String> = skill.metadata.depends_on.clone();
        drop(skills);

        path.push(name.to_string());
        let mut order = vec![];

        for dep in &deps {
            let dep_order = self.resolve_dependencies(dep, visited, path)?;
            order.extend(dep_order);
        }

        path.pop();
        visited.insert(name.to_string());
        order.push(name.to_string());
        Ok(order)
    }

    pub(in crate::registry) fn check_conflicts(
        &self,
        name: &str,
    ) -> Result<(), Error> {
        let skills = self.skills.read();
        let skill = skills.get(name).ok_or_else(|| {
            Error::NotFound(format!("skill not found: {}", name))
        })?;
        let conflicts: Vec<String> = skill.metadata.conflicts_with.clone();
        drop(skills);

        let active = self.active_skills.read();
        for conflict in &conflicts {
            if active.contains(conflict) {
                return Err(Error::InvalidItem(format!(
                    "skill '{}' conflicts with active skill '{}'",
                    name, conflict
                )));
            }
        }
        Ok(())
    }
}
