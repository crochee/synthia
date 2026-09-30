//! The catalog types the registry hands out: descriptors,
//! provenance, exposure, category, and the snapshot records.

use serde::{Deserialize, Serialize};

/// Full tool metadata for LLM tool_choice and orchestration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    /// Human-readable description for LLM.
    pub description: String,
    /// JSON Schema for tool parameters.
    pub parameters: serde_json::Value,
    /// Tool category.
    pub category: ToolCategory,
    /// Whether this tool is hidden from /help listings.
    ///
    /// See [`ToolExposure`] for how this privacy flag differs from
    /// `ToolExposure::Hidden`.
    #[serde(default)]
    pub is_hidden: bool,
    /// How much of this tool the model is shown, and when. Carried
    /// from the [`ToolEntry`](super::entry::ToolEntry) that registered it; defaults to
    /// [`ToolExposure::Direct`] so descriptor payloads written before
    /// this field existed keep deserializing.
    #[serde(default)]
    pub exposure: ToolExposure,
    /// Advisory hints from MCP [`ToolAnnotations`]. The harness
    /// reads these for permission gates and UI affordances;
    /// `None` means "the tool did not declare any". Carried
    /// additively so descriptors written before R62 keep
    /// deserializing.
    ///
    /// [`ToolAnnotations`]: https://modelcontextprotocol.io/specification/2025-06-18/server/tools#annotations
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
}

/// MCP-shaped advisory hints carried on a [`ToolDescriptor`].
///
/// All four hints are independent; a tool declares as many as it
/// can stand behind. Synthia does **not** enforce them — a
/// [`readOnlyHint = true`] tool that mutates still mutates — but
/// the harness reads them in `before_tool_call` and the operator
/// listing surfaces them. Adopting the MCP field names keeps an
/// MCP-published tool's annotations one round-trip away from
/// being projected onto the model.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    /// The tool does not modify its environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    /// The tool may have destructive side effects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    /// Calling the tool repeatedly with the same arguments has
    /// the same observable effect as calling it once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    /// The tool interacts with an open-world domain (the web, a
    /// remote service, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

impl From<ToolAnnotations> for synthia_provider::ToolAnnotations {
    fn from(value: ToolAnnotations) -> Self {
        Self {
            read_only_hint: value.read_only_hint,
            destructive_hint: value.destructive_hint,
            idempotent_hint: value.idempotent_hint,
            open_world_hint: value.open_world_hint,
        }
    }
}

/// Where a tool comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolProvenance {
    /// Built-in tool (immutable name).
    Core,
    /// Dynamically registered.
    Dynamic,
}

/// How much of a tool the model is shown, and when.
///
/// This is a *prompt-economy* knob, not a security boundary: no
/// exposure level changes what [`ToolRegistry::run_stream`](super::ToolRegistry::run_stream)(super::ToolRegistry::run_stream) will
/// dispatch. It is read by [`crate::project_tool_definitions`], the
/// single projection that builds the model-facing tool list.
///
/// ## Two levels of "visible"
///
/// `ToolExposure` and [`ToolDescriptor::is_hidden`] say different
/// things and are both honoured by the projection:
///
/// - [`ToolExposure::Hidden`] — the tool is **not advertised at all**,
///   in any form. It stays dispatcheable (a skill, a workflow step or
///   the runtime itself can still call it), exactly like a tool whose
///   [`crate::GroupedRegistry`] group is deactivated.
/// - [`ToolDescriptor::is_hidden`] (`ToolEntry::with_is_hidden`) — the
///   registry's privacy flag: the tool is not advertised **and**
///   [`ToolRegistry::run_stream`](super::ToolRegistry::run_stream) refuses a model-issued call to it.
///   The registry — not the projection — is the enforcement point.
///
/// A tool with `is_hidden = true` is therefore absent from the model
/// list no matter what its exposure says; `Hidden` exposure on a
/// non-hidden tool only removes it from the advertisement.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ToolExposure {
    /// Always visible; the full schema is sent to the LLM.
    #[default]
    Direct,
    /// Advertised as name + description with a permissive schema until
    /// the transcript shows it has been called; the full schema is sent
    /// from the next request on (see
    /// [`crate::called_tool_names`]).
    Deferred,
    /// Not advertised to the LLM; callable by the runtime only.
    Hidden,
}

/// Tool category for routing decisions.
///
/// Mirrors `synthia_core::tool::descriptor::ToolCategory` so that
/// the sub-traits can reference a category without pulling in the
/// full unified tool infrastructure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    Filesystem,
    Shell,
    Network,
    Utility,
}

/// Lightweight snapshot of a tool's definition metadata.
///
/// Cheaply cloneable, suitable for inclusion in `Vec<ToolMetadataSnapshot>`
/// in the `ToolRegistry` dual-index.
#[derive(Debug, Clone, Serialize)]
pub struct ToolMetadataSnapshot {
    pub name: String,
    pub description: String,
}

/// A snapshot of one tool's metadata plus its provenance. Returned
/// by [`ToolRegistry::snapshot_with_provenance`](super::ToolRegistry::snapshot_with_provenance).
#[derive(Debug, Clone, Serialize)]
pub struct ToolProvenanceRecord {
    pub metadata: ToolMetadataSnapshot,
    pub provenance: ToolProvenance,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R62: `ToolAnnotations` round-trips through JSON with
    /// camelCase field names (MCP wire shape).
    #[test]
    fn tool_annotations_round_trips_with_camel_case() {
        let a = ToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: None,
            open_world_hint: Some(true),
        };
        let json = serde_json::to_value(a).unwrap();
        assert_eq!(json["readOnlyHint"], serde_json::json!(true));
        assert_eq!(json["destructiveHint"], serde_json::json!(false));
        assert_eq!(json["openWorldHint"], serde_json::json!(true));
        assert!(json.get("idempotentHint").is_none());
        let back: ToolAnnotations = serde_json::from_value(json).unwrap();
        assert_eq!(back, a);
    }

    /// R62: `ToolDescriptor` deserializes without `annotations`
    /// (serde-default parity for descriptors written before R62).
    #[test]
    fn tool_descriptor_without_annotations_still_deserializes() {
        let v = serde_json::json!({
            "name": "read",
            "description": "read a file",
            "parameters": {"type": "object"},
            "category": "utility",
            "is_hidden": false,
            "exposure": "direct",
        });
        let d: ToolDescriptor = serde_json::from_value(v).unwrap();
        assert!(d.annotations.is_none());
    }

    /// R62: `From<ToolAnnotations>` for the wire type carries
    /// every hint over.
    #[test]
    fn tool_annotations_into_provider_annotations() {
        let a = ToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: None,
        };
        let w: synthia_provider::ToolAnnotations = a.into();
        assert_eq!(w.read_only_hint, Some(true));
        assert_eq!(w.destructive_hint, Some(false));
        assert_eq!(w.idempotent_hint, Some(true));
        assert_eq!(w.open_world_hint, None);
    }
}
