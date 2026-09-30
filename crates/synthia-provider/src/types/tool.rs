//! The tool-related wire types: [`ToolUse`] (LLM → agent),
//! [`ToolResult`] (agent → LLM), [`ToolDefinition`] (registered
//! tool manifest), and [`ResourceLink`] (MCP-style resource
//! reference).

#[cfg(any(feature = "anthropic", feature = "openai"))]
use base64::{
    Engine as _,
    engine::general_purpose::STANDARD as BASE64_STANDARD,
};
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

/// The name a tool call is given when it carries none.
///
/// A model can emit a `tool_use` with an empty name, and the protocol's
/// `name` field is `minLength: 1` — so the call must be labelled with
/// something rather than dropped or sent empty. Matches the placeholder
/// the OpenAI streaming processor already uses for the same situation.
#[cfg(any(feature = "anthropic", feature = "openai"))]
pub const UNNAMED_TOOL: &str = "unknown";

/// The longest tool name the Anthropic wire accepts (`name.maxLength`).
#[cfg(any(feature = "anthropic", feature = "openai"))]
const MAX_TOOL_NAME_CHARS: usize = 200;

/// Coerce a tool call's arguments to a JSON object, mapping every
/// non-object shape (`null`, scalars, arrays) to `{}`.
///
/// The single statement of the rule: an OpenAI `function.arguments` string
/// must parse to an object and Anthropic's `input` is `map[unknown]`.
#[cfg(any(feature = "anthropic", feature = "openai"))]
#[must_use]
pub(crate) fn object_or_empty(value: &Value) -> Value {
    match value {
        Value::Object(_) => value.clone(),
        Value::Null
        | Value::Bool(_)
        | Value::Number(_)
        | Value::String(_)
        | Value::Array(_) => Value::Object(serde_json::Map::new()),
    }
}

#[cfg(any(feature = "anthropic", feature = "openai"))]
impl ToolUse {
    /// The call's arguments, guaranteed to be a JSON **object**.
    ///
    /// Both adapters need this and must agree: the OpenAI `function.arguments`
    /// string has to parse to an object, and Anthropic's `input` is
    /// `map[unknown]`. A canonical input is not always one — a malformed
    /// tool call can parse to null, a scalar, or an array, which is why the
    /// OpenAI adapter has coerced and tested this since before the Anthropic
    /// adapter existed. Sharing the rule here keeps the same canonical part
    /// from being valid on one wire and a rejection on the other.
    #[must_use]
    pub(crate) fn wire_input(&self) -> Value {
        object_or_empty(&self.input)
    }

    /// The call's name, shaped for a wire whose `name` is
    /// `1 <= len <= 200`.
    ///
    /// An empty name becomes [`UNNAMED_TOOL`]. An over-long one is
    /// truncated on a character boundary rather than sent as-is: the
    /// request would otherwise be rejected outright, and the name is only
    /// ever a label here — the call is correlated by `id`, never by name,
    /// so truncating cannot mis-attribute a result.
    #[must_use]
    pub(crate) fn wire_name(&self) -> String {
        if self.name.is_empty() {
            return UNNAMED_TOOL.to_string();
        }
        if self.name.chars().count() <= MAX_TOOL_NAME_CHARS {
            return self.name.clone();
        }
        self.name.chars().take(MAX_TOOL_NAME_CHARS).collect()
    }
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
    /// MCP-shaped advisory hints (R62). `None` is omitted from the
    /// wire so existing providers that ignore `annotations` keep
    /// working unchanged.
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        rename = "annotations"
    )]
    pub annotations: Option<ToolAnnotations>,
}

/// MCP-shaped advisory hints carried on a [`ToolDefinition`].
///
/// Field names match [`crate::types::tool::ToolAnnotations`] and the
/// MCP `ToolAnnotations` spec so a server-published tool's hints
/// round-trip through the harness without translation.
#[derive(
    Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default,
)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
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
            annotations: None,
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

/// Longest `text/*` payload inlined into a prompt, in decoded bytes.
///
/// 16 KiB is roughly 4 K tokens of English — a bounded slice of any
/// context window these adapters serve, and enough for what people
/// actually attach (a note, a config, a short source file). Inlining
/// more would push the conversation itself out of the window, which is
/// the cost this projection exists to avoid.
#[cfg(any(feature = "anthropic", feature = "openai"))]
const MAX_INLINE_TEXT_BYTES: usize = 16 * 1024;

/// The encoded length that still decodes to
/// [`MAX_INLINE_TEXT_BYTES`]: base64 carries 3 bytes in 4 characters,
/// plus at most two `=` of padding. A longer payload is never decoded,
/// only measured, so a megabyte attachment costs nothing to project.
#[cfg(any(feature = "anthropic", feature = "openai"))]
const MAX_INLINE_TEXT_ENCODED: usize = MAX_INLINE_TEXT_BYTES / 3 * 4 + 4;

#[cfg(any(feature = "anthropic", feature = "openai"))]
impl ResourceLink {
    /// The bounded, human-readable note an adapter puts in a prompt in
    /// place of the resource itself.
    ///
    /// A resource normally arrives as a `data:<mime>;base64,<payload>`
    /// URI carrying the whole file — that is the shape the chat route
    /// builds from an uploaded document. Projecting that URI verbatim
    /// (which the OpenAI adapter used to do) inlines the entire base64
    /// payload: a 1 MB PDF becomes ~1.4 M characters that blow the
    /// context window and teach the model nothing. Projecting a
    /// constant (which the Anthropic adapter used to do) loses the
    /// file, filename included. Both adapters call this instead, so
    /// they agree on the *shape* of the projection while spelling
    /// their wire forms differently.
    ///
    /// The note always carries the name and media type. When the
    /// resource holds an inline `text/*` payload no longer than
    /// [`MAX_INLINE_TEXT_BYTES`], that text follows the note verbatim,
    /// so the model can read an attached note or config. Anything else
    /// — a binary media type, a payload over the limit, a `file:` or
    /// `https:` reference, a payload that is not UTF-8 — is marked
    /// *not inlined*, with its byte length when that is knowable, so
    /// the model can tell the user what it was handed instead of
    /// silently seeing nothing.
    pub(crate) fn prompt_projection(&self) -> String {
        let payload = inline_payload(&self.uri);
        let mime = self
            .mime_type
            .as_deref()
            .or_else(|| payload.map(|(mime, _)| mime))
            .unwrap_or("unknown type");
        let text = payload
            .filter(|(mime, _)| mime.starts_with("text/"))
            .and_then(|(_, data)| inline_text(data));
        match text {
            Some(text) => {
                format!("[Resource: {} ({mime})]\n{text}", self.name)
            }
            None => {
                let bytes = match payload {
                    Some((_, data)) => {
                        format!(" ({} bytes)", decoded_len(data))
                    }
                    None => String::new(),
                };
                format!(
                    "[Resource: {} ({mime}) - contents not inlined{bytes}]",
                    self.name
                )
            }
        }
    }
}

/// Split an inline `data:` URI into its media type and base64 payload.
///
/// `None` for every other shape: a `file:` / `https:` reference, a
/// `data:` URI carrying no media type, or one whose payload is not
/// base64 (this crate produces and accepts the base64 form only).
#[cfg(any(feature = "anthropic", feature = "openai"))]
fn inline_payload(uri: &str) -> Option<(&str, &str)> {
    let rest = uri.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let mime = meta.strip_suffix(";base64")?;
    (!mime.is_empty()).then_some((mime, data))
}

/// The decoded text of a base64 payload, or `None` when the payload is
/// too long to inline or does not decode to UTF-8.
#[cfg(any(feature = "anthropic", feature = "openai"))]
fn inline_text(data: &str) -> Option<String> {
    if data.len() > MAX_INLINE_TEXT_ENCODED {
        return None;
    }
    let bytes = BASE64_STANDARD.decode(data).ok()?;
    if bytes.len() > MAX_INLINE_TEXT_BYTES {
        return None;
    }
    String::from_utf8(bytes).ok()
}

/// The decoded byte length of a base64 payload, without decoding it: 4
/// encoded characters carry 3 bytes, less one byte per `=` of padding.
/// Used by the *not inlined* note, whose point is the size of what was
/// left out rather than its contents.
#[cfg(any(feature = "anthropic", feature = "openai"))]
fn decoded_len(data: &str) -> usize {
    data.len() / 4 * 3
        - data.chars().rev().take(2).filter(|c| *c == '=').count()
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

    // -- prompt_projection size bound ----------------------------------

    /// The inline-text bound is exclusive above
    /// [`MAX_INLINE_TEXT_BYTES`]: a payload that decodes to exactly
    /// the limit is inlined, one byte more is not — and the
    /// over-limit note stays short instead of carrying the payload.
    ///
    /// This is the one decision both adapters inherit, so it is pinned
    /// here rather than twice over.
    #[test]
    #[cfg(any(feature = "anthropic", feature = "openai"))]
    fn prompt_projection_inlines_text_up_to_the_bound_only() {
        use base64::{
            Engine as _,
            engine::general_purpose::STANDARD as BASE64,
        };

        let link = |bytes: usize| ResourceLink {
            uri: format!(
                "data:text/plain;base64,{}",
                BASE64.encode(vec![b'a'; bytes])
            ),
            name: "notes.txt".to_string(),
            title: None,
            description: None,
            mime_type: Some("text/plain".to_string()),
        };

        let at_limit = link(MAX_INLINE_TEXT_BYTES).prompt_projection();
        assert!(
            at_limit.contains(&"a".repeat(MAX_INLINE_TEXT_BYTES)),
            "a payload at the limit must be inlined verbatim"
        );

        let over_limit = link(MAX_INLINE_TEXT_BYTES + 1).prompt_projection();
        assert!(
            !over_limit.contains("aa"),
            "an over-limit payload must not be inlined: {over_limit}"
        );
        assert!(
            over_limit.contains("notes.txt")
                && over_limit.contains("not inlined"),
            "the note must name the file and say it was left out: {over_limit}"
        );
    }

    /// When the caller sets no `mime_type`, the media type carried by
    /// the `data:` URI itself is the one the model is told — and it is
    /// what decides whether the payload is decoded. A resource link can
    /// arrive with the envelope filled in and the field empty.
    #[test]
    #[cfg(any(feature = "anthropic", feature = "openai"))]
    fn prompt_projection_falls_back_to_the_data_uri_media_type() {
        use base64::{
            Engine as _,
            engine::general_purpose::STANDARD as BASE64,
        };

        let link = ResourceLink {
            uri: format!(
                "data:text/plain;base64,{}",
                BASE64.encode(b"hello world")
            ),
            name: "notes.txt".to_string(),
            title: None,
            description: None,
            mime_type: None,
        };
        let projection = link.prompt_projection();
        assert!(
            projection.contains("(text/plain)"),
            "the envelope's media type must be reported: {projection}"
        );
        assert!(
            projection.contains("hello world"),
            "a text payload must still be inlined: {projection}"
        );
    }
}
