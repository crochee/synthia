//! Shared tool-use streaming helpers used by both the OpenAI and
//! Anthropic provider stream processors.
//!
//! Both providers accumulate a tool call's `id`, `name`, and JSON
//! `input` as separate `delta` events stream in, then emit a
//! `ContentPart::ToolUse` chunk once the call is complete. They
//! were duplicating the `ToolUseBuffer` struct and the
//! `parse_tool_input` function byte-for-byte; the helpers now
//! live here.
//!
//! Argument finalization itself lives in [`crate::json_repair`],
//! which every finalization site (both processors and
//! [`crate::BlockAssembler`]) shares —
//! [`parse_tool_input`] is re-exported here for the existing
//! `crate::streaming::parse_tool_input` path.

use std::collections::HashMap;

pub use crate::json_repair::parse_tool_input;

/// Per-tool-call buffer used during streaming.
///
/// We keep the partial JSON as a raw `String` (not a parsed
/// `serde_json::Value`) so the delta can be emitted verbatim and
/// the parser runs once at the end inside `IsDone` / `message_stop`.
///
/// `salvaged` is `true` when the structural-close pass
/// ([`crate::json_repair::repair_json`]) salvaged a
/// mid-stream prefix the strict + repair paths could not parse.
/// The flag exists so the live-tail projection (R35 slice A) can
/// surface `"args_so_far": {...}` plus the salvage signal; the
/// final `ToolUse` value is still produced by
/// [`crate::json_repair::parse_tool_input_logged`] and is
/// unaffected by the flag.
#[derive(Debug, Clone)]
pub struct ToolUseBuffer {
    pub id: String,
    pub name: String,
    pub input: String,
    pub salvaged: bool,
}

/// Map of tool-call-id to the buffer holding its accumulated
/// partial fields. Both providers key tool calls by their
/// provider-specific index (OpenAI: numeric index; Anthropic:
/// content-block index) — keeping the map keyed by the same
/// type avoids forcing the two providers onto a common index
/// scheme.
pub type ToolUseBufferMap = HashMap<usize, ToolUseBuffer>;

#[cfg(test)]
mod tests {
    use super::*;

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
            salvaged: false,
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
                salvaged: false,
            },
        );
        map.insert(
            1,
            ToolUseBuffer {
                id: "call-1".into(),
                name: "bash".into(),
                input: "{}".into(),
                salvaged: false,
            },
        );
        assert_eq!(map.len(), 2);
        assert_eq!(map.get(&0).unwrap().name, "read_file");
        assert_eq!(map.get(&1).unwrap().name, "bash");
    }
}
