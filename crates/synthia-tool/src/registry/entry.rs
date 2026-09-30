//! The registration value type (`ToolEntry`), its passthrough
//! tool, its trait impls, and the internal storage row
//! (`ProviderEntry`) the registry keeps per name.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use synthia_core::registry::RegistryItem;

use super::{
    catalog::{ToolCategory, ToolDescriptor, ToolExposure, ToolProvenance},
    scope::RegistrationToken,
};
use crate::{
    traits::Tool,
    types::{Context, ToolOutput},
};

/// One entry in the [`ToolRegistry`](super::ToolRegistry): a type-erased [`Tool`] plus the
/// (name, description) pair the registry needs to render its catalog,
/// plus behavioural metadata that was formerly on the `Tool` trait
/// itself.
#[derive(Clone)]
pub struct ToolEntry {
    /// The underlying tool, behind an `Arc<dyn Tool>` so the registration
    /// table can share ownership cheaply with the dispatcher.
    pub(crate) tool: Arc<dyn Tool>,
    /// Cached `Tool::name()` result.
    pub(crate) name: String,
    /// Cached `Tool::description()` result.
    pub(crate) description: String,
    /// Whether the tool is hidden from user-facing listings.
    ///
    /// This is the *privacy* flag: a hidden tool is never advertised
    /// **and** [`ToolRegistry::run_stream`] refuses a model-issued call
    /// to it. For a tool that merely should not be advertised yet —
    /// but stays callable — use [`ToolExposure::Hidden`] instead.
    pub(crate) is_hidden: bool,
    /// How much of the tool the model is shown, and when.
    pub(crate) exposure: ToolExposure,
}

impl ToolEntry {
    /// Build a new entry by snapshotting `tool.name()` and
    /// `tool.description()` once, so the registry doesn't have to call
    /// them on every list/get.
    pub fn new(tool: Arc<dyn Tool>) -> Self {
        Self {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            tool,
            is_hidden: false,
            exposure: ToolExposure::Direct,
        }
    }

    /// Register a dynamic tool from raw metadata.
    ///
    /// The returned entry's `tool` is a passthrough that echoes
    /// the call arguments back to the caller. Useful for testing
    /// and for runtime registration via the
    /// `POST /api/v1/tools` endpoint. Added in turn 13 of the
    /// 2026-08-15 optimization pass.
    pub fn dynamic(
        name: String,
        description: String,
        parameters: serde_json::Value,
    ) -> Self {
        let tool = Arc::new(DynamicPassthroughTool {
            name: name.clone(),
            description: description.clone(),
            parameters,
        });
        Self {
            name,
            description,
            tool,
            is_hidden: false,
            exposure: ToolExposure::Direct,
        }
    }

    /// Return a clone of the inner `Arc<dyn Tool>` for the dispatcher to
    /// call.
    pub fn tool_instance(&self) -> Arc<dyn Tool> {
        Arc::clone(&self.tool)
    }

    /// Set whether this tool is hidden from user-facing listings.
    ///
    /// Hidden means "not advertised **and** refused at dispatch"; see
    /// [`ToolExposure`] for the softer, advertisement-only
    /// [`ToolExposure::Hidden`] level.
    pub fn with_is_hidden(mut self, val: bool) -> Self {
        self.is_hidden = val;
        self
    }

    /// Whether this tool is hidden from user-facing listings.
    pub fn is_hidden(&self) -> bool {
        self.is_hidden
    }

    /// Set how much of this tool the model is shown, and when.
    ///
    /// Builder form of [`ToolEntry::exposure`]; see [`ToolExposure`]
    /// for how `Deferred`/`Hidden` interact with
    /// [`ToolEntry::with_is_hidden`].
    #[must_use]
    pub fn with_exposure(mut self, exposure: ToolExposure) -> Self {
        self.exposure = exposure;
        self
    }

    /// How much of this tool the model is shown. [`ToolExposure::Direct`]
    /// unless [`ToolEntry::with_exposure`] said otherwise.
    #[must_use]
    pub fn exposure(&self) -> ToolExposure {
        self.exposure
    }
}

/// Tool returned by `ToolEntry::dynamic`. Echoes its arguments
/// back so the caller can verify registration without requiring
/// a side-effecting handler.
struct DynamicPassthroughTool {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[async_trait]
impl Tool for DynamicPassthroughTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> serde_json::Value {
        self.parameters.clone()
    }

    async fn call(
        &self,
        input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        ToolOutput::text(input.to_string())
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
    /// Serialise as `{name, description}` only. The `tool` field is
    /// intentionally **not** emitted — trait objects don't have a stable
    /// JSON shape, and the catalog consumers (CLI `tools list`, server
    /// introspection) only need the human metadata.
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
    /// Deserialisation is intentionally rejected — there's no portable
    /// way to rebuild a `Tool` from JSON. Callers must use
    /// [`ToolRegistry::register_entry`](super::ToolRegistry::register_entry) (which takes an `Arc<dyn Tool>`
    /// wrapped in a [`ToolEntry`]) instead of round-tripping through
    /// JSON.
    fn deserialize<D>(_deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Err(serde::de::Error::custom(
            "ToolEntry cannot be deserialized; use register_tool()",
        ))
    }
}

/// Per-provider registration record. The merged [`ToolRegistry`] stores
/// `Vec<ProviderEntry>` per tool name (LIFO ordering).
///
/// Renamed from the original `pub(crate) ToolEntry` to avoid colliding
/// with the public `ToolEntry` value type used by tool plugins
/// during registration.
#[derive(Clone)]
pub(crate) struct ProviderEntry {
    /// Token that owns this registration — used for scoped unregistration.
    pub(crate) provider_token: RegistrationToken,
    pub(crate) tool: Arc<dyn Tool>,
    pub(crate) provenance: ToolProvenance,
    pub(crate) is_hidden: bool,
    /// Exposure recorded at registration, copied onto every
    /// [`ToolDescriptor`] this entry produces.
    pub(crate) exposure: ToolExposure,
}

/// Build the public [`ToolEntry`] view of a stored [`ProviderEntry`],
/// preserving both visibility flags. Used by the `Registry::get` and
/// `Registry::list` paths so a re-materialised entry behaves exactly
/// like the one that was registered.
pub(super) fn entry_from_provider(entry: &ProviderEntry) -> ToolEntry {
    ToolEntry::new(entry.tool.clone())
        .with_is_hidden(entry.is_hidden)
        .with_exposure(entry.exposure)
}

/// Build a [`ToolDescriptor`] from a `ProviderEntry`. Reads
/// `tool.description()`, `tool.parameters()`, and the stored
/// `is_hidden` / `exposure` flags.
pub(super) fn descriptor_for(entry: &ProviderEntry) -> ToolDescriptor {
    ToolDescriptor {
        name: entry.tool.name().to_string(),
        description: entry.tool.description().to_string(),
        parameters: entry.tool.parameters(),
        category: ToolCategory::Utility,
        is_hidden: entry.is_hidden,
        exposure: entry.exposure,
        annotations: entry.tool.annotations(),
    }
}
