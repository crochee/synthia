//! `RegistryItem` and `Registry<AgentDefinition>` trait impls.
//!
//! Centralizes every trait the registry implements so the
//! call site (`super::agent_registry` for the struct,
//! `super::load` for filesystem loading, etc.) stays free of
//! trait-impl boilerplate.

use async_trait::async_trait;
use synthia_core::{
    Error,
    registry::{Registry, RegistryItem},
};

use super::{agent_registry::AgentRegistry, types::AgentDefinition};

impl RegistryItem for AgentDefinition {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }
}

#[async_trait]
impl Registry<AgentDefinition> for AgentRegistry {
    type Filter = super::types::AgentFilter;

    async fn register(
        &self,
        item: AgentDefinition,
    ) -> Result<AgentDefinition, Error> {
        let mut defs = self.definitions.write();
        if defs.contains_key(&item.id) {
            return Err(Error::AlreadyExists(item.id.clone()));
        }
        defs.insert(item.id.clone(), item.clone());
        Ok(item)
    }

    async fn unregister(&self, name: &str) -> Result<(), Error> {
        let mut defs = self.definitions.write();
        if defs.shift_remove(name).is_none() {
            return Err(Error::NotFound(name.to_string()));
        }
        Ok(())
    }

    async fn get(&self, name: &str) -> Result<Option<AgentDefinition>, Error> {
        Ok(self.definitions.read().get(name).cloned())
    }

    async fn list(
        &self,
        filter: Option<Self::Filter>,
    ) -> Result<Vec<AgentDefinition>, Error> {
        Ok(self.filter_definitions(filter))
    }
}
