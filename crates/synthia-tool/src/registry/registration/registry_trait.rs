//! `impl Registry<ToolEntry> for ToolRegistry`.
//!
//! Pulled out of [`super::registry`] so the
//! registration family keeps the same layout as the
//! other registry crates in the workspace
//! (`synthia-agent/registry/`, `synthia-skill/registry/`,
//! `synthia-mcp/manager/`) — all four follow the
//! `types` / `*_registry` / `registry_trait` / `tests`
//! shape. A new reader can land on any one of them
//! and find the trait impl in the same place.
//!
//! ## Why a separate `registry_trait` file
//!
//! The trait surface is mechanically long (4 methods:
//! `register` / `unregister` / `get` / `list`) and
//! the [`RegistryItem`] bound is the only thing
//! shared with the inherent API. Splitting it out
//! means the inherent surface in
//! [`super::registry`] stays focused on the things
//! the agent runtime actually calls
//! (`run_with_context`, `execute_tools`, the
//! `with_*` builders) and the trait surface can
//! evolve independently.

use async_trait::async_trait;
use synthia_core::{
    Error,
    registry::{Registry, RegistryItem},
};

use super::{
    super::metadata::ToolFilter,
    entry::ToolEntry,
    registry::ToolRegistry,
};
use crate::sub_traits::ToolMetadataSnapshot;

#[async_trait]
impl Registry<ToolEntry> for ToolRegistry {
    type Filter = ToolFilter;

    async fn register(
        &self,
        item: ToolEntry,
    ) -> std::result::Result<ToolEntry, Error> {
        let name = item.name().to_string();
        let mut tools = self.tools.write();
        if tools.contains_key(&name) {
            return Err(Error::AlreadyExists(name));
        }
        let meta = ToolMetadataSnapshot {
            name: item.name().to_string(),
            description: item.description().to_string(),
            category: crate::sub_traits::ToolCategory::Utility,
            parameters_schema: item.tool.parameters(),
            version: "0.1.0".to_string(),
        };
        tools.insert(name, item.clone());
        drop(tools);
        self.metadata_order.write().push(meta);
        Ok(item)
    }

    async fn unregister(&self, name: &str) -> std::result::Result<(), Error> {
        let mut tools = self.tools.write();
        tools
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| Error::NotFound(name.to_string()))?;
        drop(tools);
        let mut order = self.metadata_order.write();
        order.retain(|m| m.name != name);
        Ok(())
    }

    async fn get(
        &self,
        name: &str,
    ) -> std::result::Result<Option<ToolEntry>, Error> {
        let tools = self.tools.read();
        Ok(tools.get(name).cloned())
    }

    async fn list(
        &self,
        filter: Option<Self::Filter>,
    ) -> std::result::Result<Vec<ToolEntry>, Error> {
        let filter = filter.unwrap_or_default();
        let tools = self.tools.read();
        let result: Vec<ToolEntry> = tools
            .values()
            .filter(|entry| filter.accepts(*entry))
            .cloned()
            .collect();
        Ok(result)
    }
}
