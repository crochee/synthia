//! Shared tool-use streaming helpers used by both the OpenAI and
//! Anthropic provider stream processors.
//!
//! Both providers accumulate a tool call's `id`, `name`, and JSON
//! `input` as separate `delta` events stream in, then emit a
//! `ContentPart::ToolUse` chunk once the call is complete. They
//! were duplicating the `ToolUseBuffer` struct and the
//! `parse_tool_input` function byte-for-byte; the helpers now
//! live here.

use std::collections::HashMap;

/// Per-tool-call buffer used during streaming.
///
/// We keep the partial JSON as a raw `String` (not a parsed
/// `serde_json::Value`) so the delta can be emitted verbatim and
/// the parser runs once at the end inside `IsDone` / `message_stop`.
#[derive(Debug, Clone)]
pub struct ToolUseBuffer {
    pub id: String,
    pub name: String,
    pub input: String,
}

/// Map of tool-call-id to the buffer holding its accumulated
/// partial fields. Both providers key tool calls by their
/// provider-specific index (OpenAI: numeric index; Anthropic:
/// content-block index) — keeping the map keyed by the same
/// type avoids forcing the two providers onto a common index
/// scheme.
pub type ToolUseBufferMap = HashMap<usize, ToolUseBuffer>;

/// Best-effort parse of a tool-use argument string into a JSON value.
///
/// Both OpenAI and Anthropic guarantee valid JSON in the final
/// accumulated input (OpenAI: `tool_calls[].function.arguments`,
/// Anthropic: `input_json_delta` sequences). Mid-stream partials
/// may be unparseable; in that case return the raw string as a
/// JSON string value rather than erroring — the caller can decide
/// how to handle a malformed payload.
pub fn parse_tool_input(raw: &str) -> serde_json::Value {
    if raw.trim().is_empty() {
        return serde_json::Value::Object(serde_json::Map::new());
    }
    serde_json::from_str(raw)
        .unwrap_or_else(|_| serde_json::Value::String(raw.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `parse_tool_input` MUST return an empty
    /// object (not an empty string) for the
    /// empty input — this is the marker for
    /// "tool took no arguments". Pin the
    /// contract so a refactor that returns
    /// `Value::Null` here doesn't slip past.
    #[test]
    fn parse_tool_input_empty_string_returns_empty_object() {
        let v = parse_tool_input("");
        assert!(
            v.is_object(),
            "empty input MUST yield empty object, got: {v:?}"
        );
        assert!(v.as_object().unwrap().is_empty());
    }

    /// Whitespace-only input is treated the same
    /// as empty (no arguments) — callers rely
    /// on this when providers emit padding
    /// around tool deltas.
    #[test]
    fn parse_tool_input_whitespace_only_returns_empty_object() {
        assert!(parse_tool_input("   ").is_object());
        assert!(parse_tool_input("\t\n  ").is_object());
    }

    /// Valid JSON object MUST be parsed
    /// correctly — the fallback path is only
    /// for partial / invalid input.
    #[test]
    fn parse_tool_input_valid_object_is_parsed() {
        let v = parse_tool_input(r#"{"cmd": "ls", "path": "/"}"#);
        assert_eq!(v, serde_json::json!({"cmd": "ls", "path": "/"}));
    }

    /// Valid JSON array MUST be parsed
    /// (callers should never see array-shaped
    /// tool args, but the parser MUST still
    /// honour the type).
    #[test]
    fn parse_tool_input_valid_array_is_parsed() {
        let v = parse_tool_input(r#"[1, 2, 3]"#);
        assert_eq!(v, serde_json::json!([1, 2, 3]));
    }

    /// Valid JSON string MUST be parsed as a
    /// string Value (NOT wrapped as a
    /// fallback). Pin: the fallback only
    /// triggers on PARSE failure, not on
    /// every string-typed input.
    #[test]
    fn parse_tool_input_valid_quoted_string_is_parsed_as_string() {
        let v = parse_tool_input(r#""hello""#);
        assert_eq!(v, serde_json::Value::String("hello".into()));
    }

    /// Valid JSON number MUST be parsed as a
    /// number Value.
    #[test]
    fn parse_tool_input_valid_number_is_parsed() {
        let v = parse_tool_input("42");
        assert_eq!(v, serde_json::json!(42));
    }

    /// Mid-stream partial JSON (a tool delta
    /// ending in the middle of a value) MUST
    /// fall back to a string wrapping the raw
    /// payload, NOT panic or return `Null`.
    /// The caller decides how to handle the
    /// malformed payload.
    #[test]
    fn parse_tool_input_partial_json_falls_back_to_string() {
        let partial = r#"{"cmd": "ls""#; // missing closing brace
        let v = parse_tool_input(partial);
        match v {
            serde_json::Value::String(s) => {
                assert_eq!(s, partial);
            }
            other => panic!(
                "partial JSON MUST fall back to Value::String, got: {other:?}"
            ),
        }
    }

    /// Garbage non-JSON MUST fall back to a
    /// string wrapping the raw payload —
    /// never error, never panic.
    #[test]
    fn parse_tool_input_garbage_falls_back_to_string() {
        let v = parse_tool_input("not json at all");
        assert_eq!(v, serde_json::Value::String("not json at all".into()));
    }

    /// `ToolUseBuffer` is a 3-field data
    /// carrier (id, name, input). Pin that it
    /// `Clone`s cleanly (OpenAI stream
    /// processor snapshots it) and that its
    /// fields are stored verbatim.
    #[test]
    fn tool_use_buffer_clone_carries_all_fields_verbatim() {
        let buf = ToolUseBuffer {
            id: "call-1".to_string(),
            name: "bash".to_string(),
            input: r#"{"cmd":"ls"}"#.to_string(),
        };
        let cloned = buf.clone();
        assert_eq!(cloned.id, "call-1");
        assert_eq!(cloned.name, "bash");
        assert_eq!(cloned.input, r#"{"cmd":"ls"}"#);
    }

    /// `ToolUseBufferMap` is just a type alias
    /// for `HashMap<usize, ToolUseBuffer>`. Pin
    /// the underlying behaviour: independent
    /// keys carry independent buffers.
    #[test]
    fn tool_use_buffer_map_isolates_per_index_buffers() {
        let mut map = ToolUseBufferMap::new();
        map.insert(
            0,
            ToolUseBuffer {
                id: "call-0".into(),
                name: "read_file".into(),
                input: "{}".into(),
            },
        );
        map.insert(
            1,
            ToolUseBuffer {
                id: "call-1".into(),
                name: "bash".into(),
                input: "{}".into(),
            },
        );
        assert_eq!(map.len(), 2);
        assert_eq!(map.get(&0).unwrap().name, "read_file");
        assert_eq!(map.get(&1).unwrap().name, "bash");
    }
}
