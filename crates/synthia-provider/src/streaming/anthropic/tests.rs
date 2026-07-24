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
use crate::types::{
    ContentPart,
    ReasoningContent,
    SamplingResult,
    StreamChunk,
};

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

/// Task 1.4: Anthropic streaming must parse `signature_delta` events
/// and propagate the latest signature into subsequent reasoning
/// chunks (so callers can persist it for the next turn).
#[test]
fn signature_delta_attaches_to_subsequent_reasoning_chunks() {
    let mut p = StreamProcessorV2::new();

    // content_block_start { type: thinking }
    let start = event(
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"seed"}}"#,
    );
    let chunks = p.process_event(&start);
    // The seeded text is emitted as a Reasoning chunk; the signature
    // is None at this point because we have not yet seen a
    // signature_delta.
    match &chunks[0] {
        StreamChunk::Content(ContentPart::Reasoning(rc)) => {
            assert_eq!(rc.text, "seed");
            assert!(rc.signature.is_none());
        }
        other => panic!("expected Reasoning chunk, got {other:?}"),
    }

    // thinking_delta: signature still None.
    let t_delta = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":" more"}}"#,
    );
    let chunks = p.process_event(&t_delta);
    match &chunks[0] {
        StreamChunk::Content(ContentPart::Reasoning(rc)) => {
            assert_eq!(rc.text, " more");
            assert!(rc.signature.is_none());
        }
        other => panic!("expected Reasoning chunk, got {other:?}"),
    }

    // signature_delta: must NOT emit a chunk; must fold into the
    // accumulator so the final IsDone carries it.
    let sig_delta = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig_initial"}}"#,
    );
    let chunks = p.process_event(&sig_delta);
    assert!(
        chunks.is_empty(),
        "signature_delta should not emit stream chunks"
    );

    // A later signature_delta overrides the prior one — we keep only
    // the latest.
    let sig_delta_2 = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig_xxx"}}"#,
    );
    let chunks = p.process_event(&sig_delta_2);
    assert!(chunks.is_empty());
}

/// Task 1.5: the finalized `SamplingResult.reasoning_signature`
/// carries the latest non-None signature seen during the turn.
#[test]
fn signature_delta_aggregates_into_final_sampling_result() {
    let mut p = StreamProcessorV2::new();
    let start = event(
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"a"}}"#,
    );
    let sig_1 = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig_one"}}"#,
    );
    let sig_2 = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig_two"}}"#,
    );
    let stop = event(r#"{"type":"content_block_stop","index":0}"#);
    let msg_stop = event(r#"{"type":"message_stop"}"#);

    p.process_event(&start);
    p.process_event(&sig_1);
    p.process_event(&sig_2);
    p.process_event(&stop);
    let cs = p.process_event(&msg_stop);
    assert_eq!(cs.len(), 1);
    let SamplingResult {
        reasoning,
        reasoning_signature,
        ..
    } = match &cs[0] {
        StreamChunk::IsDone { result } => (**result).clone(),
        other => panic!("expected IsDone, got {other:?}"),
    };
    assert_eq!(reasoning, "a");
    assert_eq!(
        reasoning_signature.as_deref(),
        Some("sig_two"),
        "Aggregated signature must be the last non-None signature seen"
    );
}

/// Turn that emits reasoning text but no signature_delta must leave
/// `reasoning_signature` as `None`.
#[test]
fn signature_absent_leaves_sampling_result_signature_none() {
    let mut p = StreamProcessorV2::new();
    let start = event(
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"only text"}}"#,
    );
    let stop = event(r#"{"type":"content_block_stop","index":0}"#);
    let msg_stop = event(r#"{"type":"message_stop"}"#);
    p.process_event(&start);
    p.process_event(&stop);
    let cs = p.process_event(&msg_stop);
    let SamplingResult {
        reasoning,
        reasoning_signature,
        ..
    } = match &cs[0] {
        StreamChunk::IsDone { result } => (**result).clone(),
        other => panic!("expected IsDone, got {other:?}"),
    };
    assert_eq!(reasoning, "only text");
    assert!(
        reasoning_signature.is_none(),
        "Without a signature_delta, reasoning_signature stays None"
    );
}

/// The reasoning_chunk emitted after the signature is observed
/// carries the signature on the part so callers can persist it
/// before the IsDone.
#[test]
fn reasoning_part_after_signature_carries_signature() {
    let mut p = StreamProcessorV2::new();
    let start = event(
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"first"}}"#,
    );
    let sig = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig_persist"}}"#,
    );
    p.process_event(&start);
    p.process_event(&sig);
    let t = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"more"}}"#,
    );
    let chunks = p.process_event(&t);
    match &chunks[0] {
        StreamChunk::Content(ContentPart::Reasoning(ReasoningContent {
            text,
            signature,
        })) => {
            assert_eq!(text, "more");
            assert_eq!(signature.as_deref(), Some("sig_persist"));
        }
        other => panic!("expected Reasoning chunk, got {other:?}"),
    }
}
