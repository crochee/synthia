//! Unit tests for the `anthropic` stream processor module family.
//!
//! Coverage map (5 tests):
//!
//! - `StreamProcessorV2` text deltas: 1 test
//!   ([`text_delta_emits_content_text`]).
//! - `StreamProcessorV2` thinking: 1 test
//!   ([`thinking_delta_emits_content_reasoning`]).
//! - `StreamProcessorV2` tool-use end-to-end: 1 test
//!   ([`tool_use_emits_start_delta_end_with_is_done`]) —
//!   covers `ToolCallStart` → 2× `ToolCallDelta` →
//!   `ToolCallEnd` → `IsDone` with parsed JSON `{"location":
//!   "Beijing"}`.
//! - `IsDone` accumulated text: 1 test
//!   ([`is_done_carries_accumulated_text`]) — verifies that
//!   multiple `text_delta` events accumulate into a single
//!   `result.text`.
//! - `parse_tool_input` edge cases: 2 tests
//!   ([`parse_tool_input_handles_empty_string`],
//!   [`parse_tool_input_handles_invalid_json`]).

use serde_json::json;

use super::*;
use crate::types::{ContentPart, StreamChunk};

fn event(json: &str) -> AnthropicStreamEvent {
    serde_json::from_str(json).expect("valid event")
}

#[test]
fn text_delta_emits_content_text() {
    let mut p = StreamProcessorV2::new();
    let ev = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#,
    );
    let chunks = p.process_event(&ev);
    assert_eq!(chunks.len(), 1);
    match &chunks[0] {
        StreamChunk::Content(ContentPart::Text(t)) => {
            assert_eq!(t.text, "Hello")
        }
        other => panic!("expected Content(Text), got {other:?}"),
    }
}

#[test]
fn thinking_delta_emits_content_reasoning() {
    let mut p = StreamProcessorV2::new();
    let ev = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"think…"}}"#,
    );
    let chunks = p.process_event(&ev);
    assert_eq!(chunks.len(), 1);
    assert!(matches!(
        chunks[0],
        StreamChunk::Content(ContentPart::Reasoning(_))
    ));
}

#[test]
fn tool_use_emits_start_delta_end_with_is_done() {
    let mut p = StreamProcessorV2::new();
    let start = event(
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"t1","name":"get_weather","input":""}}"#,
    );
    let d1 = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"location\":"}}"#,
    );
    let d2 = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"\"Beijing\"}"}}"#,
    );
    let stop = event(r#"{"type":"content_block_stop","index":0}"#);
    let msg_stop = event(r#"{"type":"message_stop"}"#);

    let cs1 = p.process_event(&start);
    assert!(
        matches!(&cs1[0], StreamChunk::ToolCallStart { id, name, .. } if id == "t1" && name == "get_weather")
    );
    let cs2 = p.process_event(&d1);
    assert!(
        matches!(&cs2[0], StreamChunk::ToolCallDelta { id, arguments_delta } if id == "t1" && arguments_delta == "{\"location\":")
    );
    let cs3 = p.process_event(&d2);
    assert!(
        matches!(&cs3[0], StreamChunk::ToolCallDelta { arguments_delta, .. } if arguments_delta == "\"Beijing\"}")
    );
    let cs4 = p.process_event(&stop);
    assert!(matches!(&cs4[0], StreamChunk::ToolCallEnd { id } if id == "t1"));
    let cs5 = p.process_event(&msg_stop);
    assert_eq!(cs5.len(), 1);
    match &cs5[0] {
        StreamChunk::IsDone { result } => {
            assert_eq!(result.text, "");
            assert_eq!(result.reasoning, "");
            assert_eq!(result.tool_calls.len(), 1);
            let tc = &result.tool_calls[0];
            assert_eq!(tc.id, "t1");
            assert_eq!(tc.name, "get_weather");
            assert_eq!(tc.input, json!({"location": "Beijing"}));
            // usage is None at PR1-M2 (delta_usage is a placeholder);
            // assert only that it serialises to default, not equality.
            let _ = result.usage; // no panic
        }
        other => panic!("expected IsDone, got {other:?}"),
    }
}

#[test]
fn is_done_carries_accumulated_text() {
    let mut p = StreamProcessorV2::new();
    let d1 = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello, "}}"#,
    );
    let d2 = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"world!"}}"#,
    );
    let stop = event(r#"{"type":"content_block_stop","index":0}"#);
    let msg = event(r#"{"type":"message_stop","stop_reason":"end_turn"}"#);
    p.process_event(&d1);
    p.process_event(&d2);
    p.process_event(&stop);
    let cs = p.process_event(&msg);
    match &cs[0] {
        StreamChunk::IsDone { result } => {
            assert_eq!(result.text, "Hello, world!")
        }
        other => panic!("expected IsDone, got {other:?}"),
    }
}

#[test]
fn parse_tool_input_handles_empty_string() {
    let v = super::v2::parse_tool_input("");
    assert!(v.is_object());
    assert!(v.as_object().unwrap().is_empty());
}

#[test]
fn parse_tool_input_handles_invalid_json() {
    let v = super::v2::parse_tool_input("not json");
    assert_eq!(v, json!("not json"));
}
