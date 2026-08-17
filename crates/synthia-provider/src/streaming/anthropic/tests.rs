//! Unit tests for the `anthropic` stream processor module family.
//!
//! Coverage map:
//!
//! - `StreamProcessor` text deltas: 1 test
//!   ([`text_delta_emits_content_text`]).
//! - `StreamProcessor` thinking: 1 test
//!   ([`thinking_delta_emits_content_reasoning`]).
//! - `StreamProcessor` tool-use end-to-end: 1 test
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
//! - Orphan tool-use buffer (truncated SSE): 1 test
//!   ([`message_stop_with_orphan_tool_buffer_drops_tool_call`]).

use serde_json::json;

use super::*;
use crate::{
    streaming::parse_tool_input,
    types::{ContentPart, ReasoningContent, SamplingResult, StreamChunk},
};

fn event(json: &str) -> AnthropicStreamEvent {
    serde_json::from_str(json).expect("valid event")
}

#[test]
fn text_delta_emits_content_text() {
    let mut p = StreamProcessor::new();
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

/// `text_delta` arrives as multiple chunks per turn —
/// the canonical Anthropic streaming shape is one
/// `text_delta` per token. The processor MUST push
/// every chunk into `self.text` so the final
/// `IsDone.text` carries the concatenated full
/// response, AND emit each chunk as its own
/// `StreamChunk::Content(Text)` so the frontend sees
/// a streaming typewriter. Without the per-delta
/// emit, the frontend would receive no chunks until
/// `message_stop`. Without the per-delta push, the
/// final result would only carry the LAST delta.
/// Pins the dual-emit contract.
#[test]
fn text_delta_accumulates_across_deltas_into_final_result() {
    let mut p = StreamProcessor::new();
    // Three text_deltas — exercise the concatenation path.
    let d1 = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello "}}"#,
    );
    let d2 = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"world "}}"#,
    );
    let d3 = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"!"}}"#,
    );
    let stop = event(r#"{"type":"content_block_stop","index":0}"#);
    let msg_stop = event(r#"{"type":"message_stop"}"#);

    // Each delta emits its own Content(Text) chunk —
    // streaming typewriter contract.
    let cs1 = p.process_event(&d1);
    let cs2 = p.process_event(&d2);
    let cs3 = p.process_event(&d3);
    for cs in [&cs1, &cs2, &cs3] {
        assert_eq!(cs.len(), 1);
        assert!(
            cs.iter().any(|c| matches!(
                c,
                StreamChunk::Content(ContentPart::Text(_))
            )),
            "each text_delta must emit its own Content(Text); got {cs:?}"
        );
    }
    p.process_event(&stop);
    let cs_done = p.process_event(&msg_stop);
    let result = match &cs_done[0] {
        StreamChunk::IsDone { result } => (**result).clone(),
        other => panic!("expected IsDone, got {other:?}"),
    };
    assert_eq!(
        result.text, "Hello world !",
        "text_delta chunks must accumulate into the final result; got {:?}",
        result.text
    );
}

/// Anthropic's API allows multiple `text` content
/// blocks in a single message (e.g. when the model
/// emits reasoning text followed by a final
/// answer). The processor MUST concatenate them all
/// into `self.text` so the final result carries the
/// full response — even though each block has its
/// own `content_block_start` / `content_block_stop`.
/// Without this contract, a 2-block response would
/// keep only the LAST block's text in the
/// `SamplingResult`, silently dropping the first.
#[test]
fn text_accumulates_across_multiple_content_blocks() {
    let mut p = StreamProcessor::new();
    // Block 0: "First "
    let b0_start = event(
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
    );
    let b0_d = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"First "}}"#,
    );
    let b0_stop = event(r#"{"type":"content_block_stop","index":0}"#);
    // Block 1: "Second"
    let b1_start = event(
        r#"{"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}"#,
    );
    let b1_d = event(
        r#"{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Second"}}"#,
    );
    let b1_stop = event(r#"{"type":"content_block_stop","index":1}"#);
    let msg_stop = event(r#"{"type":"message_stop"}"#);

    p.process_event(&b0_start);
    p.process_event(&b0_d);
    p.process_event(&b0_stop);
    p.process_event(&b1_start);
    p.process_event(&b1_d);
    p.process_event(&b1_stop);
    let cs_done = p.process_event(&msg_stop);
    let result = match &cs_done[0] {
        StreamChunk::IsDone { result } => (**result).clone(),
        other => panic!("expected IsDone, got {other:?}"),
    };
    assert_eq!(
        result.text, "First Second",
        "text across multiple content blocks must concatenate; got {:?}",
        result.text
    );
}

#[test]
fn thinking_delta_emits_content_reasoning() {
    let mut p = StreamProcessor::new();
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

/// `thinking_delta` arrives as multiple chunks per
/// turn — Anthropic streams one chunk per token of
/// reasoning text. The processor MUST push every
/// chunk into `self.reasoning` so the final
/// `IsDone.reasoning` carries the full reasoning
/// trace (used for cross-turn reasoning continuity
/// when the agent injects it into the next prompt).
/// Each chunk must ALSO emit its own
/// `StreamChunk::Content(Reasoning)` so the frontend
/// shows the reasoning live. Without the dual
/// emit/push contract, reasoning continuity breaks
/// OR the frontend sees no live reasoning.
#[test]
fn thinking_delta_accumulates_across_deltas_into_final_result() {
    let mut p = StreamProcessor::new();
    // Open a thinking block first.
    let start = event(
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#,
    );
    p.process_event(&start);
    // Three thinking_deltas.
    let d1 = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"step 1 "}}"#,
    );
    let d2 = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"step 2 "}}"#,
    );
    let d3 = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"step 3"}}"#,
    );
    let stop = event(r#"{"type":"content_block_stop","index":0}"#);
    let msg_stop = event(r#"{"type":"message_stop"}"#);

    // Each delta emits its own Content(Reasoning)
    // chunk — streaming live-reasoning contract.
    for (ev, expected_text) in
        [(&d1, "step 1 "), (&d2, "step 2 "), (&d3, "step 3")]
    {
        let cs = p.process_event(ev);
        assert_eq!(cs.len(), 1);
        match &cs[0] {
            StreamChunk::Content(ContentPart::Reasoning(r)) => {
                assert_eq!(r.text, *expected_text);
            }
            other => panic!("expected Content(Reasoning), got {other:?}"),
        }
    }
    p.process_event(&stop);
    let cs_done = p.process_event(&msg_stop);
    let result = match &cs_done[0] {
        StreamChunk::IsDone { result } => (**result).clone(),
        other => panic!("expected IsDone, got {other:?}"),
    };
    assert_eq!(
        result.reasoning, "step 1 step 2 step 3",
        "thinking_delta chunks must accumulate into the final result; got {:?}",
        result.reasoning
    );
}

/// Two parallel `tool_use` content blocks (different
/// `index` values, different ids) MUST accumulate
/// independently — the per-index buffers are keyed by
/// `index`, not by `id`, and both must end up in the
/// final `IsDone.tool_calls` vec in the order their
/// `content_block_stop` arrived. Anthropic emits
/// parallel tool calls in this shape when the model
/// requests multiple actions in a single turn. Without
/// this contract, the second tool would clobber the
/// first or get dropped entirely.
#[test]
fn two_parallel_tool_use_blocks_emit_independent_streams() {
    let mut p = StreamProcessor::new();

    // Start block index=0 (id=t1, name=get_weather).
    let start0 = event(
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"t1","name":"get_weather","input":""}}"#,
    );
    // Start block index=1 (id=t2, name=lookup_time).
    let start1 = event(
        r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"t2","name":"lookup_time","input":""}}"#,
    );
    // Interleaved deltas — exercise the per-index routing.
    let d0 = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"location\":\"Beijing\"}"}}"#,
    );
    let d1 = event(
        r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"tz\":\"UTC\"}"}}"#,
    );
    let stop0 = event(r#"{"type":"content_block_stop","index":0}"#);
    let stop1 = event(r#"{"type":"content_block_stop","index":1}"#);
    let msg_stop = event(r#"{"type":"message_stop"}"#);

    // Both starts must emit ToolCallStart.
    let cs0 = p.process_event(&start0);
    let cs1 = p.process_event(&start1);
    assert!(
        matches!(&cs0[0], StreamChunk::ToolCallStart { id, .. } if id == "t1")
    );
    assert!(
        matches!(&cs1[0], StreamChunk::ToolCallStart { id, .. } if id == "t2")
    );

    // Interleaved deltas — each routes to its own buffer.
    let csd0 = p.process_event(&d0);
    let csd1 = p.process_event(&d1);
    assert!(
        matches!(&csd0[0], StreamChunk::ToolCallDelta { id, .. } if id == "t1")
    );
    assert!(
        matches!(&csd1[0], StreamChunk::ToolCallDelta { id, .. } if id == "t2")
    );

    // Both stops must emit ToolCallEnd with their own ids.
    let css0 = p.process_event(&stop0);
    let css1 = p.process_event(&stop1);
    assert!(matches!(&css0[0], StreamChunk::ToolCallEnd { id } if id == "t1"));
    assert!(matches!(&css1[0], StreamChunk::ToolCallEnd { id } if id == "t2"));

    // Final IsDone must carry BOTH tool calls with
    // correct parsed inputs, in content_block_stop order.
    let cs_done = p.process_event(&msg_stop);
    let result = match &cs_done[0] {
        StreamChunk::IsDone { result } => (**result).clone(),
        other => panic!("expected IsDone, got {other:?}"),
    };
    assert_eq!(result.tool_calls.len(), 2);
    assert_eq!(result.tool_calls[0].id, "t1");
    assert_eq!(result.tool_calls[0].name, "get_weather");
    assert_eq!(result.tool_calls[0].input, json!({"location": "Beijing"}));
    assert_eq!(result.tool_calls[1].id, "t2");
    assert_eq!(result.tool_calls[1].name, "lookup_time");
    assert_eq!(result.tool_calls[1].input, json!({"tz": "UTC"}));
}

#[test]
fn tool_use_emits_start_delta_end_with_is_done() {
    let mut p = StreamProcessor::new();
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
    let mut p = StreamProcessor::new();
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
    let v = parse_tool_input("");
    assert!(v.is_object());
    assert!(v.as_object().unwrap().is_empty());
}

#[test]
fn parse_tool_input_handles_invalid_json() {
    let v = parse_tool_input("not json");
    assert_eq!(v, json!("not json"));
}

/// Task 1.4: Anthropic streaming must parse `signature_delta` events
/// and propagate the latest signature into subsequent reasoning
/// chunks (so callers can persist it for the next turn).
#[test]
fn signature_delta_attaches_to_subsequent_reasoning_chunks() {
    let mut p = StreamProcessor::new();

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
    let mut p = StreamProcessor::new();
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
    let mut p = StreamProcessor::new();
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
    let mut p = StreamProcessor::new();
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

/// Regression test: a `tool_use` content block whose
/// `content_block_stop` is never observed (truncated SSE,
/// network drop, or a malformed upstream that skips the
/// closing event) MUST NOT appear in the final
/// `SamplingResult.tool_calls`. The downstream tool
/// dispatcher relies on this invariant — a half-parsed
/// tool call with truncated input JSON would either fail
/// the tool's schema or execute with empty arguments,
/// producing confusing user-facing errors.
///
/// The processor MUST:
/// 1. emit `ToolCallStart` on `content_block_start` (the
///    `ToolCallStart` already passed through at this
///    point),
/// 2. emit `ToolCallDelta` for any `input_json_delta` that
///    arrived before the stream ended,
/// 3. drop the orphan buffer at `message_stop` (no
///    `ToolCallEnd`, no `IsDone` tool entry),
/// 4. emit an `IsDone` with an empty `tool_calls` vec.
#[test]
fn message_stop_with_orphan_tool_buffer_drops_tool_call() {
    let mut p = StreamProcessor::new();

    // 1. content_block_start for tool_use — emits
    //    ToolCallStart.
    let start = event(
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_orphan","name":"get_weather"}}"#,
    );
    let cs = p.process_event(&start);
    assert!(
        cs.iter()
            .any(|c| matches!(c, StreamChunk::ToolCallStart { .. })),
        "ToolCallStart must fire on content_block_start; got {cs:?}"
    );

    // 2. input_json_delta arrives — emits ToolCallDelta.
    let delta = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"location\":\"Be"}}"#,
    );
    let cs = p.process_event(&delta);
    assert!(
        cs.iter()
            .any(|c| matches!(c, StreamChunk::ToolCallDelta { .. })),
        "ToolCallDelta must fire on input_json_delta; got {cs:?}"
    );

    // NO `content_block_stop` for index 0. Stream jumps
    // straight to `message_stop`.

    // 3. message_stop — must NOT produce a ToolCallEnd for
    //    the orphan, and the final IsDone.tool_calls must
    //    be empty.
    let msg_stop = event(r#"{"type":"message_stop"}"#);
    let cs = p.process_event(&msg_stop);
    let tool_end_emitted = cs
        .iter()
        .any(|c| matches!(c, StreamChunk::ToolCallEnd { .. }));
    assert!(
        !tool_end_emitted,
        "ToolCallEnd must NOT fire for an orphan tool buffer; got {cs:?}"
    );
    let is_done = cs.iter().find_map(|c| match c {
        StreamChunk::IsDone { result } => Some((**result).clone()),
        _ => None,
    });
    let is_done = is_done.expect("message_stop must emit IsDone");
    assert!(
        is_done.tool_calls.is_empty(),
        "IsDone.tool_calls must be empty when no content_block_stop arrived; got {:?}",
        is_done.tool_calls
    );
}

/// `message_delta` carries the turn-level `stop_reason`
/// but not the per-event `stop_reason` shape of
/// `message_stop`. The processor MUST propagate the
/// `stop_reason` from `message_delta` into the final
/// `SamplingResult` so downstream consumers can
/// distinguish "end_turn" from "tool_use" from "max_tokens"
/// without parsing the SSE themselves. This pins down
/// the `self.stop_reason = Some(reason.clone())` path.
#[test]
fn message_delta_stop_reason_is_propagated_to_final_result() {
    let mut p = StreamProcessor::new();
    // Some text content first.
    let start = event(
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
    );
    let delta = event(
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}"#,
    );
    let stop = event(r#"{"type":"content_block_stop","index":0}"#);
    // message_delta with stop_reason="max_tokens".
    // Anthropic carries stop_reason at the top level of the
    // message_delta event (not inside `delta`); see
    // streaming/anthropic/events.rs::AnthropicStreamEvent.
    let msg_delta =
        event(r#"{"type":"message_delta","stop_reason":"max_tokens"}"#);
    let msg_stop = event(r#"{"type":"message_stop"}"#);
    p.process_event(&start);
    p.process_event(&delta);
    p.process_event(&stop);
    p.process_event(&msg_delta);
    let cs = p.process_event(&msg_stop);
    let result = match &cs[0] {
        StreamChunk::IsDone { result } => (**result).clone(),
        other => panic!("expected IsDone, got {other:?}"),
    };
    assert_eq!(result.text, "hi");
    assert_eq!(
        result.stop_reason.as_deref(),
        Some("max_tokens"),
        "message_delta.stop_reason must be captured; got {:?}",
        result.stop_reason
    );
}

/// Anthropic emits `redacted_thinking` content
/// blocks (not `thinking_delta` deltas) when the
/// model emits reasoning that the safety filter
/// later redacts. The block arrives whole — no
/// per-token deltas — and the processor MUST surface
/// it as a `Content(Reasoning)` chunk with a fixed
/// "[Redacted by safety filter]" marker, AND fold it
/// into `self.reasoning` so the final
/// `IsDone.reasoning` preserves the (placeholder)
/// reasoning trace. Without this contract, the
/// downstream agent would lose track of reasoning
/// boundaries when redacted blocks appear between
/// normal thinking blocks.
///
/// Test doc explicitly: redacted_thinking arrives at
/// `content_block_start` (not as deltas), and the
/// marker text is hard-coded to keep the safety
/// contract symmetric — we never expose the
/// redacted raw bytes.
#[test]
fn redacted_thinking_block_emits_marker_and_folds_into_reasoning() {
    let mut p = StreamProcessor::new();
    let start = event(
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"redacted_thinking"}}"#,
    );
    let cs = p.process_event(&start);
    assert_eq!(cs.len(), 1);
    match &cs[0] {
        StreamChunk::Content(ContentPart::Reasoning(r)) => {
            assert_eq!(
                r.text, "[Redacted by safety filter]",
                "redacted_thinking must surface the fixed marker; got {:?}",
                r.text
            );
            // No signature is ever attached to a
            // redacted block (the original signature
            // is what got redacted in the first place).
            assert!(
                r.signature.is_none(),
                "redacted_thinking must not carry a signature; got {:?}",
                r.signature
            );
        }
        other => panic!("expected Content(Reasoning), got {other:?}"),
    }
    // Close the block and the stream.
    let stop = event(r#"{"type":"content_block_stop","index":0}"#);
    p.process_event(&stop);
    let msg_stop = event(r#"{"type":"message_stop"}"#);
    let cs_done = p.process_event(&msg_stop);
    let result = match &cs_done[0] {
        StreamChunk::IsDone { result } => (**result).clone(),
        other => panic!("expected IsDone, got {other:?}"),
    };
    assert_eq!(
        result.reasoning, "[Redacted by safety filter]",
        "redacted_thinking marker must accumulate into IsDone.reasoning; got {:?}",
        result.reasoning
    );
    assert!(
        result.reasoning_signature.is_none(),
        "redacted_thinking must leave reasoning_signature as None"
    );
}

/// Multiple redacted_thinking blocks accumulate
/// their markers (one per block) into the final
/// reasoning field. Each block emits its own
/// `Content(Reasoning)` chunk so the frontend can
/// show redacted sections separately. Without this
/// contract, two redacted blocks would collapse to
/// one marker in the final result.
#[test]
fn multiple_redacted_thinking_blocks_accumulate_independently() {
    let mut p = StreamProcessor::new();
    // Block 0: redacted.
    p.process_event(&event(
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"redacted_thinking"}}"#,
    ));
    p.process_event(&event(r#"{"type":"content_block_stop","index":0}"#));
    // Block 1: another redacted.
    p.process_event(&event(
        r#"{"type":"content_block_start","index":1,"content_block":{"type":"redacted_thinking"}}"#,
    ));
    p.process_event(&event(r#"{"type":"content_block_stop","index":1}"#));
    // Block 2: normal text — gives the stream a
    // visible completion signal.
    let txt_start = event(
        r#"{"type":"content_block_start","index":2,"content_block":{"type":"text","text":""}}"#,
    );
    let txt_d = event(
        r#"{"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"hi"}}"#,
    );
    let txt_stop = event(r#"{"type":"content_block_stop","index":2}"#);
    p.process_event(&txt_start);
    p.process_event(&txt_d);
    p.process_event(&txt_stop);
    let msg_stop = event(r#"{"type":"message_stop"}"#);
    let cs_done = p.process_event(&msg_stop);
    let result = match &cs_done[0] {
        StreamChunk::IsDone { result } => (**result).clone(),
        other => panic!("expected IsDone, got {other:?}"),
    };
    assert_eq!(
        result.reasoning,
        "[Redacted by safety filter][Redacted by safety filter]",
        "two redacted blocks must concatenate their markers; got {:?}",
        result.reasoning
    );
    assert_eq!(
        result.text, "hi",
        "text from the visible block must be unaffected; got {:?}",
        result.text
    );
}

/// `message_delta` may arrive with **no** stop_reason
/// (e.g. mid-stream usage-only updates). The processor
/// MUST NOT clobber a previously-captured stop_reason
/// with None, AND MUST NOT panic on None. This pins
/// the `if let Some(reason) = ...` guard.
#[test]
fn message_delta_without_stop_reason_does_not_clobber() {
    let mut p = StreamProcessor::new();
    let start = event(
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
    );
    let stop = event(r#"{"type":"content_block_stop","index":0}"#);
    // First message_delta sets stop_reason.
    let msg_delta_1 =
        event(r#"{"type":"message_delta","stop_reason":"end_turn"}"#);
    // Second message_delta has NO stop_reason (only
    // usage-style fields). The inner `delta` needs a
    // `type` field because AnthropicStreamDelta is a
    // required-field struct.
    let msg_delta_2 = event(r#"{"type":"message_delta","delta":{"type":""}}"#);
    let msg_stop = event(r#"{"type":"message_stop"}"#);
    p.process_event(&start);
    p.process_event(&stop);
    p.process_event(&msg_delta_1);
    p.process_event(&msg_delta_2);
    let cs = p.process_event(&msg_stop);
    let result = match &cs[0] {
        StreamChunk::IsDone { result } => (**result).clone(),
        other => panic!("expected IsDone, got {other:?}"),
    };
    assert_eq!(
        result.stop_reason.as_deref(),
        Some("end_turn"),
        "stop_reason from the first message_delta must persist; got {:?}",
        result.stop_reason
    );
}
