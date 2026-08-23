//! The tool-related wire types: [`ToolUse`] (LLM → agent),
//! [`ToolResult`] (agent → LLM), [`ToolDefinition`] (registered
//! tool manifest), and [`ResourceLink`] (MCP-style resource
//! reference).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::content::{ContentPart, TextContent};
use crate::cache_mark::CacheControlMark;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    pub tool_use_id: String,
    /// Optional tool name. Carried on the wire so downstream
    /// consumers (SSE mapping, frontend segment rendering) can
    /// label a `tool_result` even when no preceding `tool_call`
    /// segment exists in the same message (e.g. results flushed
    /// by an interceptor or replayed from session JSONL).
    /// `#[serde(default)]` keeps deserialization backward-compatible
    /// with payloads written before this field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    pub content: Vec<ContentPart>,
    pub structured_content: Option<Value>,
    pub is_error: Option<bool>,
    /// Structured metadata accompanying the tool result (counts,
    /// timing, truncation reason). Defaults to empty for
    /// backward compatibility with payloads written before this
    /// field existed.
    ///
    /// The field is forwarded from `synthia_tool::ToolOutput`'s
    /// `metadata` so the LLM and the frontend
    /// can see tool-attached telemetry without parsing the
    /// content stream.
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub metadata: serde_json::Map<String, Value>,
    /// Optional truncation reason — populated when the
    /// orchestrator or the tool itself trimmed the output
    /// before returning it to the LLM. Forwarded from
    /// `synthia_tool::ToolOutput::truncated_by` so downstream
    /// consumers know the result was bounded and how to find
    /// the full text (e.g. `SpilledTo.path`).
    ///
    /// Stored as a generic JSON value to keep this crate
    /// independent of `synthia_tool` (which defines the
    /// `TruncatedBy` enum). The agent loop converts the
    /// `synthia_tool::types::TruncatedBy` into its
    /// `serde_json::Value` representation before constructing
    /// the wire `ToolResult`, preserving full round-trip
    /// information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated_by: Option<Value>,
}

impl ToolResult {
    pub fn new(
        tool_use_id: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            tool_use_id: tool_use_id.into(),
            tool_name: None,
            content: vec![ContentPart::Text(TextContent {
                text: text.into(),
                cache_control: None,
            })],
            structured_content: None,
            is_error: None,
            metadata: serde_json::Map::new(),
            truncated_by: None,
        }
    }

    pub fn error(
        tool_use_id: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            tool_use_id: tool_use_id.into(),
            tool_name: None,
            content: vec![ContentPart::Text(TextContent {
                text: text.into(),
                cache_control: None,
            })],
            structured_content: None,
            is_error: Some(true),
            metadata: serde_json::Map::new(),
            truncated_by: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cache_control: Option<CacheControlMark>,
}

impl ToolDefinition {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            cache_control: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLink {
    pub uri: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::content::ContentPart;

    // -- ToolResult::new + ::error constructors -----------------------

    /// `ToolResult::new` MUST construct a
    /// well-formed result: tool_use_id
    /// verbatim, single TextContent
    /// populated, is_error=None,
    /// metadata empty, truncated_by=None.
    #[test]
    fn tool_result_new_builds_text_content_with_error_none() {
        let r = ToolResult::new("call-1", "the output");
        assert_eq!(r.tool_use_id, "call-1");
        assert!(r.tool_name.is_none());
        assert_eq!(r.content.len(), 1);
        match &r.content[0] {
            ContentPart::Text(t) => assert_eq!(t.text, "the output"),
            _ => panic!("expected Text content"),
        }
        assert!(r.is_error.is_none());
        assert!(r.metadata.is_empty());
        assert!(r.truncated_by.is_none());
        assert!(r.structured_content.is_none());
    }

    /// `ToolResult::error` MUST set
    /// `is_error = Some(true)` so callers can
    /// distinguish failed results from
    /// success without parsing content.
    #[test]
    fn tool_result_error_sets_is_error_true() {
        let r = ToolResult::error("call-2", "command failed");
        assert_eq!(r.tool_use_id, "call-2");
        assert_eq!(r.is_error, Some(true));
        assert_eq!(r.content.len(), 1);
    }

    // -- ToolResult serde forward-compat ------------------------------

    /// `ToolResult` MUST round-trip
    /// `metadata` and `truncated_by`
    /// verbatim. These fields are
    /// forward-compat additions; pin so a
    /// refactor that drops them breaks
    /// loudly.
    #[test]
    fn tool_result_metadata_and_truncated_by_round_trip_through_json() {
        let mut r = ToolResult::new("call-3", "ok");
        r.metadata
            .insert("bytes".to_string(), serde_json::json!(4096));
        r.metadata
            .insert("elapsed_ms".to_string(), serde_json::json!(12));
        r.truncated_by = Some(serde_json::json!({
            "kind": "SpilledTo",
            "path": "/tmp/spill.jsonl"
        }));
        let json = serde_json::to_string(&r).unwrap();
        // Both fields present in wire output.
        assert!(json.contains("\"metadata\""), "got: {json}");
        assert!(json.contains("\"truncated_by\""), "got: {json}");
        let parsed: ToolResult =
            serde_json::from_str(&json).expect("round-trip parse");
        assert_eq!(
            parsed.metadata.get("bytes"),
            Some(&serde_json::json!(4096))
        );
        assert_eq!(
            parsed.truncated_by,
            Some(serde_json::json!({
                "kind": "SpilledTo",
                "path": "/tmp/spill.jsonl"
            }))
        );
    }

    /// Old payloads without `metadata` /
    /// `truncated_by` MUST still deserialize
    /// (forward-compat). Both fields use
    /// `#[serde(default)]`.
    #[test]
    fn tool_result_old_payload_without_metadata_or_truncated_by_deserializes() {
        let old_json = r#"{
            "tool_use_id": "call-old",
            "content": [{"type": "text", "text": "ok"}]
        }"#;
        let parsed: ToolResult =
            serde_json::from_str(old_json).expect("parse old payload");
        assert_eq!(parsed.tool_use_id, "call-old");
        assert!(parsed.metadata.is_empty());
        assert!(parsed.truncated_by.is_none());
        assert!(parsed.tool_name.is_none());
    }

    // -- ToolDefinition -------------------------------------------------

    /// `ToolDefinition::new` MUST construct
    /// with cache_control=None by default.
    #[test]
    fn tool_definition_new_starts_with_cache_control_none() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "cmd": {"type": "string"}
            }
        });
        let def =
            ToolDefinition::new("bash", "Run a shell command", schema.clone());
        assert_eq!(def.name, "bash");
        assert_eq!(def.description, "Run a shell command");
        assert_eq!(def.input_schema, schema);
        assert!(def.cache_control.is_none());
    }

    /// `ToolDefinition::cache_control`
    /// (when `Some`) MUST serialize with
    /// `skip_serializing_if = "Option::is_none"`
    /// so absent values stay absent in the
    /// wire JSON (forward-compat).
    #[test]
    fn tool_definition_omits_cache_control_when_none_in_json() {
        let def = ToolDefinition::new("bash", "d", serde_json::json!({}));
        let json = serde_json::to_string(&def).unwrap();
        assert!(
            !json.contains("cacheControl"),
            "absent cache_control MUST NOT appear in JSON: {json}"
        );
        assert!(
            !json.contains("cache_control"),
            "absent cache_control MUST NOT appear in JSON: {json}"
        );
    }

    // -- ResourceLink camelCase ----------------------------------------

    /// `ResourceLink` MUST serialize
    /// `mime_type` as `mimeType` (camelCase)
    /// per MCP wire convention. Pin
    /// so a refactor that drops the rename
    /// breaks compatibility.
    #[test]
    fn resource_link_serializes_mime_type_as_camel_case() {
        let link = ResourceLink {
            uri: "file:///tmp/x.txt".to_string(),
            name: "x.txt".to_string(),
            title: Some("X file".to_string()),
            description: None,
            mime_type: Some("text/plain".to_string()),
        };
        let json = serde_json::to_string(&link).unwrap();
        assert!(
            json.contains("\"mimeType\":\"text/plain\""),
            "mimeType MUST be camelCase: {json}"
        );
        assert!(
            !json.contains("mime_type"),
            "snake_case MUST NOT appear: {json}"
        );
    }

    /// `ResourceLink` MUST tolerate old
    /// snake_case payloads during
    /// deserialization. (camelCase is for
    /// OUTGOING serialization; deser uses
    /// #[serde(alias)]? — actually it does
    /// not, so this test pins the
    /// INVARIANT that the round-trip is
    /// exact and old clients must use the
    /// same naming.)
    #[test]
    fn resource_link_round_trips_via_camel_case() {
        let link = ResourceLink {
            uri: "https://example.com/r".to_string(),
            name: "r".to_string(),
            title: None,
            description: Some("desc".to_string()),
            mime_type: None,
        };
        let json = serde_json::to_string(&link).unwrap();
        let parsed: ResourceLink =
            serde_json::from_str(&json).expect("ResourceLink round-trip parse");
        assert_eq!(parsed.uri, "https://example.com/r");
        assert_eq!(parsed.name, "r");
        assert_eq!(parsed.description, Some("desc".to_string()));
        assert!(parsed.mime_type.is_none());
    }
}
