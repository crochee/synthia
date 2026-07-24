//! Definition-level read queries — `filter_definitions` /
//! `contains` / `len` / `is_empty`.
//!
//! These operate on the read-only `definitions` map (as
//! opposed to [`super::instances`], which operates on the
//! live instances). Used by both the `Registry<AgentDefinition>`
//! trait impl and direct callers.

use super::{
    agent_registry::AgentRegistry,
    types::{AgentDefinition, AgentFilter},
};

impl AgentRegistry {
    /// Apply an optional [`AgentFilter`] to the loaded
    /// definitions and return the result sorted by name.
    pub(super) fn filter_definitions(
        &self,
        filter: Option<AgentFilter>,
    ) -> Vec<AgentDefinition> {
        let defs = self.definitions.read();
        let filter = filter.unwrap_or_default();

        let mut result: Vec<_> = defs
            .values()
            .filter(|def| {
                if !def.enabled && filter.enabled_only {
                    return false;
                }
                if let Some(ref name) = filter.name
                    && !def.name.to_lowercase().contains(&name.to_lowercase())
                {
                    return false;
                }
                if let Some(ref cap) = filter.capability
                    && !def
                        .capabilities
                        .iter()
                        .any(|c| c.to_lowercase().contains(&cap.to_lowercase()))
                {
                    return false;
                }
                true
            })
            .cloned()
            .collect();

        result.sort_by(|a, b| a.name.cmp(&b.name));
        result
    }

    /// Whether the registry contains a definition with the
    /// given id.
    pub fn contains(&self, name: &str) -> bool {
        self.definitions.read().contains_key(name)
    }

    /// Number of loaded definitions.
    pub fn len(&self) -> usize {
        self.definitions.read().len()
    }

    /// Whether the registry has no loaded definitions.
    pub fn is_empty(&self) -> bool {
        self.definitions.read().is_empty()
    }
}
