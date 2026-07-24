//! [`ToolEntry`] — the value type the tool registry
//! stores.
//!
//! One entry wraps a single [`Tool`] (kept behind
//! `Arc<dyn Tool>` to avoid the per-call vtable clone
//! cost) plus its name + description. The description
//! is duplicated onto the entry so that
//! [`Registry::list`] / JSON serialisation can render
//! the catalog without dereferencing the trait object.
//!
//! The `RegistryItem` / `Serialize` / `Deserialize`
//! impls all live here, kept with the data they
//! describe, instead of being spread across the rest
//! of the registration family.

use std::sync::Arc;

use serde::Serialize;
use synthia_core::registry::RegistryItem;

use crate::traits::Tool;

/// One entry in the [`super::ToolRegistry`]: a
/// type-erased [`Tool`] plus the (name, description)
/// pair the registry needs to render its catalog.
#[derive(Clone)]
pub struct ToolEntry {
    /// The underlying tool, behind an `Arc<dyn Tool>`
    /// so the registration table can share ownership
    /// cheaply with the dispatcher. `pub(super)` so
    /// the dispatcher in [`super::registry`] can
    /// read it (for `is_hidden()` checks) and the
    /// trait impl in [`super::registry_trait`] can
    /// clone it (for `get`).
    pub(super) tool: Arc<dyn Tool>,
    /// Cached `Tool::name()` result.
    pub(super) name: String,
    /// Cached `Tool::description()` result.
    pub(super) description: String,
}

impl ToolEntry {
    /// Build a new entry by snapshotting `tool.name()`
    /// and `tool.description()` once, so the registry
    /// doesn't have to call them on every list/get.
    pub fn new(tool: Arc<dyn Tool>) -> Self {
        Self {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            tool,
        }
    }

    /// Return a clone of the inner `Arc<dyn Tool>` for
    /// the dispatcher to call.
    pub fn tool_instance(&self) -> Arc<dyn Tool> {
        Arc::clone(&self.tool)
    }
}

impl RegistryItem for ToolEntry {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }
}

impl Serialize for ToolEntry {
    /// Serialise as `{name, description}` only. The
    /// `tool` field is intentionally **not** emitted —
    /// trait objects don't have a stable JSON shape,
    /// and the catalog consumers (CLI `tools list`,
    /// server introspection) only need the human
    /// metadata.
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("ToolEntry", 2)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("description", &self.description)?;
        state.end()
    }
}

impl<'de> serde::Deserialize<'de> for ToolEntry {
    /// Deserialisation is intentionally rejected —
    /// there's no portable way to rebuild a
    /// `Tool` from JSON. Callers must use
    /// [`super::ToolRegistry::register`] (which takes
    /// an `Arc<dyn Tool>` directly) instead of
    /// round-tripping through JSON.
    fn deserialize<D>(_deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Err(serde::de::Error::custom(
            "ToolEntry cannot be deserialized; use register_tool()",
        ))
    }
}
