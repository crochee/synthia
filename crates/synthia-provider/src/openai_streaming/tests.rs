use serde_json::json;

use super::*;
use crate::{
    streaming::parse_tool_input,
    types::{ContentPart, ReasoningContent, StreamChunk},
};

#[test]
fn content_delta_emits_text_chunk() {
    let mut p = OpenAIStreamProcessor::new();
    let line = r#"{"id":"x","choices":[{"delta":{"content":"Hello"}},{"finish_reason":null}]}"#;
    let cs = p.process_line(line);
    assert_eq!(cs.len(), 1);
    match &cs[0] {
        StreamChunk::Content(ContentPart::Text(t)) => {
            assert_eq!(t.text, "Hello")
        }
        other => panic!("expected Content(Text), got {other:?}"),
    }
}

#[test]
fn reasoning_content_emits_reasoning_chunk_no_sniffing() {
    let mut p = OpenAIStreamProcessor::new();
    let line = r#"{"id":"x","choices":[{"delta":{"reasoning_content":"think… thinkingstill thinking"}}]}"#;
    let cs = p.process_line(line);
    assert_eq!(cs.len(), 1);
    match &cs[0] {
        StreamChunk::Content(ContentPart::Reasoning(t)) => {
            // The full raw string is passed through; we do NOT
            // sniff out  thinking markers (Bug 1 fix).
            assert_eq!(t.text, "think… thinkingstill thinking")
        }
        other => panic!("expected Content(Reasoning), got {other:?}"),
    }
}

/// `reasoning_content` arrives in multiple deltas and
/// MUST accumulate into `self.reasoning` so the final
/// `SamplingResult.reasoning` field carries the full
/// concatenated reasoning text. Without this contract,
/// a multi-delta reasoning stream would leak only the
/// last delta into the result, dropping the earlier
/// reasoning segments. Mirrors the text-content
/// accumulation contract (also tested via the
/// `is_done_carries_accumulated_text` shape).
#[test]
fn reasoning_content_accumulates_across_deltas() {
    let mut p = OpenAIStreamProcessor::new();
    let d1 =
        r#"{"id":"x","choices":[{"delta":{"reasoning_content":"first "}}]}"#;
    let d2 =
        r#"{"id":"x","choices":[{"delta":{"reasoning_content":"second "}}]}"#;
    let d3 =
        r#"{"id":"x","choices":[{"delta":{"reasoning_content":"third"}}]}"#;
    let cs1 = p.process_line(d1);
    let cs2 = p.process_line(d2);
    let cs3 = p.process_line(d3);
    // Each delta emits its own chunk (no sniffing).
    for cs in [&cs1, &cs2, &cs3] {
        assert_eq!(cs.len(), 1);
        assert!(
            cs.iter().any(|c| matches!(
                c,
                StreamChunk::Content(ContentPart::Reasoning(_))
            )),
            "each reasoning delta must emit a Reasoning chunk; got {cs:?}"
        );
    }
    // Close via finish_reason.
    let end = r#"{"id":"x","choices":[{"finish_reason":"stop"}]}"#;
    let cs_end = p.process_line(end);
    let is_done = cs_end
        .iter()
        .find_map(|c| match c {
            StreamChunk::IsDone { result } => Some((**result).clone()),
            _ => None,
        })
        .expect("IsDone must fire on finish_reason");
    assert_eq!(
        is_done.reasoning, "first second third",
        "reasoning_content must accumulate; got {:?}",
        is_done.reasoning
    );
}

/// Empty `reasoning_content` deltas must NOT pollute
/// the accumulated reasoning with empty strings. The
/// processor filters `!reasoning.is_empty()` per-delta;
/// this test pins that the filter actually works by
/// interleaving empty deltas with non-empty ones.
#[test]
fn empty_reasoning_content_delta_does_not_pollute_accumulator() {
    let mut p = OpenAIStreamProcessor::new();
    p.process_line(
        r#"{"id":"x","choices":[{"delta":{"reasoning_content":"hello "}}]}"#,
    );
    // Empty string — must be ignored.
    p.process_line(
        r#"{"id":"x","choices":[{"delta":{"reasoning_content":""}}]}"#,
    );
    p.process_line(
        r#"{"id":"x","choices":[{"delta":{"reasoning_content":"world"}}]}"#,
    );
    let end = r#"{"id":"x","choices":[{"finish_reason":"stop"}]}"#;
    let cs = p.process_line(end);
    let is_done = cs
        .iter()
        .find_map(|c| match c {
            StreamChunk::IsDone { result } => Some((**result).clone()),
            _ => None,
        })
        .expect("IsDone must fire");
    assert_eq!(
        is_done.reasoning, "hello world",
        "empty reasoning_content must be filtered; got {:?}",
        is_done.reasoning
    );
}

#[test]
fn tool_call_emits_start_delta_end_with_is_done() {
    let mut p = OpenAIStreamProcessor::new();
    let d1 = r#"{"id":"x","choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"get_weather","arguments":"{\"loc"}}]}}]}"#;
    let d2 = r#"{"id":"x","choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ation\":\"Beijing\"}"}}]}}]}"#;
    let end = r#"{"id":"x","choices":[{"finish_reason":"tool_calls"}]}"#;

    let cs1 = p.process_line(d1);
    assert!(matches!(
        &cs1[0],
        StreamChunk::ToolCallStart { id, name, .. } if id == "call_1" && name == "get_weather"
    ));
    // d1 also contained an initial argument — that should have been
    // included in the start chunk's `arguments` and buffered, but
    // no Delta is emitted (Delta is for "subsequent" partials).
    let cs2 = p.process_line(d2);
    assert!(matches!(
        &cs2[0],
        StreamChunk::ToolCallDelta { id, arguments_delta } if id == "call_1" && arguments_delta == "ation\":\"Beijing\"}"
    ));
    let cs3 = p.process_line(end);
    // end should emit ToolCallEnd + IsDone
    let end_chunk = cs3
        .iter()
        .find(|c| matches!(c, StreamChunk::ToolCallEnd { .. }))
        .expect("ToolCallEnd present");
    let is_done = cs3
        .iter()
        .find(|c| matches!(c, StreamChunk::IsDone { .. }))
        .expect("IsDone present");
    match end_chunk {
        StreamChunk::ToolCallEnd { id } => assert_eq!(id, "call_1"),
        _ => unreachable!(),
    }
    match is_done {
        StreamChunk::IsDone { result } => {
            assert_eq!(result.text, "");
            assert_eq!(result.reasoning, "");
            assert_eq!(result.tool_calls.len(), 1);
            let tc = &result.tool_calls[0];
            assert_eq!(tc.id, "call_1");
            assert_eq!(tc.name, "get_weather");
            assert_eq!(tc.input, json!({"location": "Beijing"}));
        }
        _ => unreachable!(),
    }
}

#[test]
fn parse_tool_input_handles_empty_string() {
    let v = parse_tool_input("");
    assert!(v.is_object());
    assert!(v.as_object().unwrap().is_empty());
}

/// Usage chunks arrive on a final choice with empty
/// delta and `finish_reason: null` when the provider
/// has `stream_options.include_usage` set. The
/// processor MUST emit a `StreamChunk::Usage` so live
/// dashboards can show the token count, AND propagate
/// the usage to the final `IsDone.result.usage` so
/// downstream consumers see the canonical final value.
/// This test pins down both contracts on the happy
/// path: usage chunk arrives, IsDone fires (via a
/// follow-up `[DONE]`), and the IsDone result carries
/// the same token counts.
#[test]
fn usage_chunk_is_emitted_and_propagates_to_is_done() {
    let mut p = OpenAIStreamProcessor::new();
    // Some text content first.
    let text = r#"{"id":"x","choices":[{"delta":{"content":"hi"}}]}"#;
    p.process_line(text);
    // Usage-only choice (empty delta, finish_reason: null).
    let usage_line = r#"{"id":"x","choices":[{"delta":{},"finish_reason":null,"usage":{"prompt_tokens":42,"completion_tokens":7,"total_tokens":49}}]}"#;
    let cs = p.process_line(usage_line);
    let usage_chunk = cs
        .iter()
        .find_map(|c| match c {
            StreamChunk::Usage(u) => Some(u.clone()),
            _ => None,
        })
        .expect("Usage chunk must be emitted on usage-bearing choice");
    assert_eq!(usage_chunk.prompt_tokens, 42);
    assert_eq!(usage_chunk.completion_tokens, 7);
    assert_eq!(usage_chunk.total_tokens, 49);
    // Stream terminates via [DONE].
    let cs_end = p.process_line("[DONE]");
    let is_done = cs_end
        .iter()
        .find_map(|c| match c {
            StreamChunk::IsDone { result } => Some((**result).clone()),
            _ => None,
        })
        .expect("IsDone must fire on [DONE]");
    assert_eq!(is_done.usage.prompt_tokens, 42);
    assert_eq!(is_done.usage.completion_tokens, 7);
    assert_eq!(is_done.usage.total_tokens, 49);
}

/// When the stream terminates WITHOUT a usage chunk
/// (provider doesn't support `include_usage`), the
/// `IsDone.result.usage` MUST default to all-zeros —
/// NOT panic, NOT propagate a stale value from a
/// previous turn, NOT skip the IsDone emission. This
/// pins the `unwrap_or_default()` fallback in
/// `emit_is_done`.
#[test]
fn is_done_usage_defaults_to_zero_when_no_usage_chunk_arrived() {
    let mut p = OpenAIStreamProcessor::new();
    p.process_line(r#"{"id":"x","choices":[{"delta":{"content":"hi"}}]}"#);
    let cs = p.process_line("[DONE]");
    let is_done = cs
        .iter()
        .find_map(|c| match c {
            StreamChunk::IsDone { result } => Some((**result).clone()),
            _ => None,
        })
        .expect("IsDone must fire on [DONE]");
    assert_eq!(is_done.usage.prompt_tokens, 0);
    assert_eq!(is_done.usage.completion_tokens, 0);
    assert_eq!(is_done.usage.total_tokens, 0);
}

/// When multiple usage chunks arrive (rare, but some
/// OpenAI-compatible gateways re-send usage on each
/// tool-call retry), the **last** one wins for the
/// `IsDone.result.usage` field, but each usage chunk
/// still produces a `StreamChunk::Usage` event so live
/// dashboards update continuously. This test pins
/// down the "last-wins" contract.
#[test]
fn multiple_usage_chunks_use_last_wins_for_is_done() {
    let mut p = OpenAIStreamProcessor::new();
    let usage1 = r#"{"id":"x","choices":[{"delta":{},"finish_reason":null,"usage":{"prompt_tokens":10,"completion_tokens":1,"total_tokens":11}}]}"#;
    p.process_line(usage1);
    let usage2 = r#"{"id":"x","choices":[{"delta":{},"finish_reason":null,"usage":{"prompt_tokens":99,"completion_tokens":9,"total_tokens":108}}]}"#;
    let cs = p.process_line(usage2);
    // The second usage chunk must emit its own Usage
    // event (not silently overwritten).
    let usage_chunk = cs
        .iter()
        .find_map(|c| match c {
            StreamChunk::Usage(u) => Some(u.clone()),
            _ => None,
        })
        .expect("Usage chunk must be emitted on every usage-bearing choice");
    assert_eq!(usage_chunk.prompt_tokens, 99);
    // Close via finish_reason.
    let cs_end =
        p.process_line(r#"{"id":"x","choices":[{"finish_reason":"stop"}]}"#);
    let is_done = cs_end
        .iter()
        .find_map(|c| match c {
            StreamChunk::IsDone { result } => Some((**result).clone()),
            _ => None,
        })
        .expect("IsDone must fire");
    // Last-wins: the IsDone result carries the SECOND
    // usage chunk's values.
    assert_eq!(is_done.usage.prompt_tokens, 99);
    assert_eq!(is_done.usage.completion_tokens, 9);
    assert_eq!(is_done.usage.total_tokens, 108);
}

#[test]
fn parse_tool_input_handles_invalid_json() {
    let v = parse_tool_input("not json");
    assert_eq!(v, json!("not json"));
}

/// Inline `<think>…</think>` markers in `delta.content` get split into
/// a Reasoning chunk followed by a Text chunk so the downstream
/// mapping can route each piece through the correct `kind`.
#[test]
fn inline_think_marker_in_content_is_split() {
    let mut p = OpenAIStreamProcessor::new();
    let line = r#"{"id":"x","choices":[{"delta":{"content":"<think>deep thought</think>answer"}}]}"#;
    let cs = p.process_line(line);
    let mut reasoning = String::new();
    let mut text = String::new();
    for chunk in &cs {
        match chunk {
            StreamChunk::Content(ContentPart::Reasoning(r)) => {
                reasoning.push_str(&r.text);
            }
            StreamChunk::Content(ContentPart::Text(t)) => {
                text.push_str(&t.text);
            }
            other => panic!("expected Content chunk, got {other:?}"),
        }
    }
    assert_eq!(reasoning, "deep thought");
    assert_eq!(text, "answer");
}

/// Multiple deltas that together form a single `<think>…</think>`
/// block: each delta is split only after the marker boundary is
/// fully visible (carry semantics). The first delta ends with the
/// partial opener `"<thin"`; the second delta completes `"<think>"`.
#[test]
fn inline_think_marker_split_across_deltas() {
    let mut p = OpenAIStreamProcessor::new();
    let d1 = r#"{"id":"x","choices":[{"delta":{"content":"hello <thin"}}]}"#;
    let d2 = r#"{"id":"x","choices":[{"delta":{"content":"k>reason</think>done"}}]}"#;

    let cs1 = p.process_line(d1);
    let cs2 = p.process_line(d2);
    let cs: Vec<StreamChunk> = cs1.into_iter().chain(cs2).collect();

    let mut reasoning = String::new();
    let mut text = String::new();
    for chunk in &cs {
        match chunk {
            StreamChunk::Content(ContentPart::Reasoning(r)) => {
                reasoning.push_str(&r.text);
            }
            StreamChunk::Content(ContentPart::Text(t)) => {
                text.push_str(&t.text);
            }
            other => panic!("expected Content chunk, got {other:?}"),
        }
    }
    assert_eq!(reasoning, "reason");
    assert_eq!(text, "hello done");
}

/// At `IsDone` the extractor flushes any text or reasoning the
/// carry was withholding so the aggregated `SamplingResult` is
/// complete.
#[test]
fn extract_residual_carry_on_is_done() {
    let mut p = OpenAIStreamProcessor::new();
    // Last six characters `" <thin"` form a partial `<think>` opener
    // — carried over and only released by the flush.
    p.process_line(
        r#"{"id":"x","choices":[{"delta":{"content":"answer <thin"}}]}"#,
    );
    let cs =
        p.process_line(r#"{"id":"x","choices":[{"finish_reason":"stop"}]}"#);
    let is_done = cs
        .iter()
        .find(|c| matches!(c, StreamChunk::IsDone { .. }))
        .expect("IsDone present");
    match is_done {
        StreamChunk::IsDone { result } => {
            assert_eq!(result.text, "answer <thin");
        }
        other => panic!("expected IsDone, got {other:?}"),
    }
}

#[test]
fn done_token_emits_is_done() {
    let mut p = OpenAIStreamProcessor::new();
    let line = r#"{"id":"x","choices":[{"delta":{"content":"hi"}}]}"#;
    p.process_line(line);
    // Caller strips the `data:` prefix before calling process_line.
    let cs = p.process_line("[DONE]");
    assert_eq!(cs.len(), 1);
    match &cs[0] {
        StreamChunk::IsDone { result } => assert_eq!(result.text, "hi"),
        other => panic!("expected IsDone, got {other:?}"),
    }
}

/// Regression test: when a stream ends via `[DONE]` WITHOUT
/// a preceding `finish_reason` chunk (malformed upstream,
/// truncation, or a provider that uses `[DONE]` as the
/// sole terminal signal), any tool call whose buffer was
/// opened via a `tool_calls` delta but never closed MUST
/// still appear in `IsDone.tool_calls`. The
/// `finish_reason`-driven happy path drains
/// `tool_buffers` and emits `ToolCallEnd` BEFORE calling
/// `emit_is_done`; the `[DONE]`-driven fallback path
/// skips that drain, so this test pins the fallback
/// contract that closes the gap.
///
/// Without this contract, an OpenAI variant that
/// truncates mid-tool-call would silently lose the
/// tool_use, and the agent loop would see an empty
/// `tool_calls` vec in `IsDone` even though
/// `ToolCallStart` + `ToolCallDelta` chunks had been
/// streamed. Mirrors the anthropic
/// `message_stop_with_orphan_tool_buffer_drops_tool_call`
/// test on the OpenAI side.
#[test]
fn done_after_tool_delta_flushes_orphan_buffer_into_tool_calls() {
    let mut p = OpenAIStreamProcessor::new();
    // Open a tool_use buffer via a tool_calls delta.
    let d1 = r#"{"id":"x","choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_x","type":"function","function":{"name":"get_weather","arguments":"{\"loc"}}]}}]}"#;
    let cs1 = p.process_line(d1);
    assert!(
        cs1.iter()
            .any(|c| matches!(c, StreamChunk::ToolCallStart { .. })),
        "ToolCallStart must fire; got {cs1:?}"
    );
    // Append a delta. Buffer is now live but never
    // closed via finish_reason.
    let d2 = r#"{"id":"x","choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ation\":\"Beijing\"}"}}]}}]}"#;
    p.process_line(d2);
    // Stream ends with [DONE] and NO `finish_reason`.
    let cs = p.process_line("[DONE]");
    let is_done = cs
        .iter()
        .find_map(|c| match c {
            StreamChunk::IsDone { result } => Some((**result).clone()),
            _ => None,
        })
        .expect("IsDone must fire on [DONE]");
    assert_eq!(
        is_done.tool_calls.len(),
        1,
        "orphan tool_use must be flushed into IsDone.tool_calls; got {:?}",
        is_done.tool_calls
    );
    let tc = &is_done.tool_calls[0];
    assert_eq!(tc.id, "call_x");
    assert_eq!(tc.name, "get_weather");
    assert_eq!(tc.input, json!({"location": "Beijing"}));
}

/// Regression test: when the **first** tool_calls delta
/// has `id = None`, the processor synthesizes a stable
/// fallback id of the form `call_{index}` so the
/// downstream tool-call dispatcher always has *some* id
/// to key off. This pins the contract that the buffer's
/// `id` is never empty, never `None`, and matches the
/// `id` emitted in `ToolCallStart` (downstream consumers
/// use that id to dedupe `ToolCallEnd` events). Without
/// this contract, an upstream that omits `id` on the
/// first delta would produce a `ToolUse { id: "" }`
/// which is rejected by most tool dispatchers.
#[test]
fn tool_call_without_id_on_first_delta_uses_index_fallback() {
    let mut p = OpenAIStreamProcessor::new();
    // First delta has NO `id` field at all.
    let d1 = r#"{"id":"x","choices":[{"delta":{"tool_calls":[{"index":0,"type":"function","function":{"name":"get_weather","arguments":"{\"loc"}}]}}]}"#;
    let cs1 = p.process_line(d1);
    let start = cs1
        .iter()
        .find_map(|c| match c {
            StreamChunk::ToolCallStart { id, name, .. } => {
                Some((id.clone(), name.clone()))
            }
            _ => None,
        })
        .expect("ToolCallStart must fire");
    assert_eq!(
        start.0, "call_0",
        "missing id on first delta must fall back to \"call_{{index}}\"; got {:?}",
        start.0
    );
    assert_eq!(start.1, "get_weather");

    // Close via finish_reason.
    let end = r#"{"id":"x","choices":[{"finish_reason":"tool_calls"}]}"#;
    let cs3 = p.process_line(end);
    let is_done = cs3
        .iter()
        .find_map(|c| match c {
            StreamChunk::IsDone { result } => Some((**result).clone()),
            _ => None,
        })
        .expect("IsDone must fire");
    assert_eq!(is_done.tool_calls.len(), 1);
    // The same fallback id propagates to the final ToolUse.
    assert_eq!(
        is_done.tool_calls[0].id, "call_0",
        "ToolCallEnd id and final ToolUse.id must match the synthesized fallback"
    );
}

/// Regression test: a **later** delta that carries a
/// `name` patch (rare but allowed by the wire format —
/// some providers re-send the full name as a
/// confirmation) MUST be patched into the buffer. The
/// patch is a **full replacement**, not an append — the
/// wire format always sends the canonical name when it
/// does appear in any delta. Without the patch, a
/// second-delta rename (e.g. provider bug that first
/// emits `name = ""` and later sends the real name)
/// would silently keep the empty placeholder in the
/// final `IsDone.tool_calls`.
#[test]
fn tool_call_name_is_patched_by_subsequent_delta() {
    let mut p = OpenAIStreamProcessor::new();
    // First delta has an empty placeholder name.
    let d1 = r#"{"id":"x","choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_n","type":"function","function":{"name":""}}]}}]}"#;
    p.process_line(d1);
    // Second delta supplies the real name. The patch
    // REPLACES, not appends.
    let d2 = r#"{"id":"x","choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"get_weather"}}]}}]}"#;
    p.process_line(d2);
    // Close.
    let end = r#"{"id":"x","choices":[{"finish_reason":"tool_calls"}]}"#;
    let cs3 = p.process_line(end);
    let is_done = cs3
        .iter()
        .find_map(|c| match c {
            StreamChunk::IsDone { result } => Some((**result).clone()),
            _ => None,
        })
        .expect("IsDone must fire");
    assert_eq!(is_done.tool_calls.len(), 1);
    assert_eq!(
        is_done.tool_calls[0].name, "get_weather",
        "name patches must replace, not append; got {:?}",
        is_done.tool_calls[0].name
    );
}

/// Task 1.7: OpenAI streaming produces reasoning chunks without a
/// signature (OpenAI doesn't emit one). The reasoning part and the
/// final `SamplingResult` must both leave `signature` / `reasoning_signature`
/// as `None`.
#[test]
fn openai_reasoning_chunks_leave_signature_none() {
    let mut p = OpenAIStreamProcessor::new();
    let line = r#"{"id":"x","choices":[{"delta":{"reasoning_content":"thinking through this"}}]}"#;
    let cs = p.process_line(line);
    assert_eq!(cs.len(), 1);
    match &cs[0] {
        StreamChunk::Content(ContentPart::Reasoning(ReasoningContent {
            text,
            signature,
        })) => {
            assert_eq!(text, "thinking through this");
            assert!(
                signature.is_none(),
                "OpenAI must leave the part's signature as None"
            );
        }
        other => panic!("expected Content(Reasoning), got {other:?}"),
    }

    // Drive the processor to IsDone so we can also assert the
    // aggregated SamplingResult leaves reasoning_signature as None.
    let end_line = r#"{"id":"x","choices":[{"finish_reason":"stop"}]}"#;
    let cs = p.process_line(end_line);
    let is_done = cs
        .iter()
        .find(|c| matches!(c, StreamChunk::IsDone { .. }))
        .expect("IsDone present");
    match is_done {
        StreamChunk::IsDone { result } => {
            assert_eq!(result.reasoning, "thinking through this");
            assert!(
                result.reasoning_signature.is_none(),
                "OpenAI stream must aggregate reasoning_signature as None"
            );
        }
        other => panic!("expected IsDone, got {other:?}"),
    }
}

/// A single OpenAI delta may carry BOTH
/// `reasoning_content` and `content` — for example,
/// o1-style models surface their chain-of-thought in
/// `reasoning_content` while still streaming the
/// user-visible answer in `content`. The processor
/// MUST emit both chunks in the same `process_line`
/// invocation and accumulate both into the final
/// `IsDone` — without this contract, the visible
/// answer would race against the reasoning or, worse,
/// one would be silently dropped. Pins down that the
/// two `if let Some(...)` blocks at lines 77 and 88
/// coexist rather than short-circuiting.
#[test]
fn reasoning_content_and_content_coexist_in_same_delta() {
    let mut p = OpenAIStreamProcessor::new();
    let line = r#"{"id":"x","choices":[{"delta":{"reasoning_content":"think...","content":"visible answer"}}]}"#;
    let cs = p.process_line(line);
    // Two chunks: one Content(Reasoning), one Content(Text)
    assert_eq!(
        cs.len(),
        2,
        "single delta with both reasoning_content and content must emit two chunks; got {cs:?}"
    );
    let has_reasoning = cs
        .iter()
        .any(|c| matches!(c, StreamChunk::Content(ContentPart::Reasoning(_))));
    let has_text = cs
        .iter()
        .any(|c| matches!(c, StreamChunk::Content(ContentPart::Text(_))));
    assert!(
        has_reasoning && has_text,
        "both Reasoning and Text chunks must be present; got {cs:?}"
    );
    // Drive to IsDone and assert BOTH fields populated.
    let end_line = r#"{"id":"x","choices":[{"finish_reason":"stop"}]}"#;
    let cs_done = p.process_line(end_line);
    let result = match cs_done
        .iter()
        .find(|c| matches!(c, StreamChunk::IsDone { .. }))
    {
        Some(StreamChunk::IsDone { result }) => (**result).clone(),
        Some(other) => panic!("expected IsDone, got {other:?}"),
        None => panic!("no IsDone chunk emitted"),
    };
    assert_eq!(result.reasoning, "think...");
    assert_eq!(result.text, "visible answer");
}

/// A single OpenAI delta may carry BOTH visible
/// `content` and a `tool_calls` array — this is the
/// canonical streaming shape when the model emits a
/// short narration alongside a tool invocation
/// (e.g. "Looking up the weather for you…" +
/// tool_call(get_weather)). The processor MUST emit
/// the text chunk AND the tool chunks in the same
/// `process_line` call — without this contract, the
/// narration would race against the tool, or one
/// would be silently dropped. Pins down that the
/// `content` block (line 88) and the `tool_calls`
/// block (line 110) coexist rather than
/// short-circuiting.
#[test]
fn content_and_tool_calls_coexist_in_same_delta() {
    let mut p = OpenAIStreamProcessor::new();
    let line = r#"{"id":"x","choices":[{"delta":{"content":"Looking up...","tool_calls":[{"index":0,"id":"call_w","type":"function","function":{"name":"get_weather","arguments":"{\"city\":\"Beijing\"}"}}]}}]}"#;
    let cs = p.process_line(line);
    // Must contain: 1 Content(Text) + 1 ToolCallStart.
    assert_eq!(
        cs.len(),
        2,
        "single delta with both content and tool_calls must emit two chunks; got {cs:?}"
    );
    let has_text = cs
        .iter()
        .any(|c| matches!(c, StreamChunk::Content(ContentPart::Text(_))));
    let has_start = cs
        .iter()
        .any(|c| matches!(c, StreamChunk::ToolCallStart { .. }));
    assert!(
        has_text && has_start,
        "both Content(Text) and ToolCallStart must be present; got {cs:?}"
    );
    // Drive to IsDone and assert both fields populated.
    let end_line = r#"{"id":"x","choices":[{"finish_reason":"tool_calls"}]}"#;
    let cs_done = p.process_line(end_line);
    let result = match cs_done
        .iter()
        .find(|c| matches!(c, StreamChunk::IsDone { .. }))
    {
        Some(StreamChunk::IsDone { result }) => (**result).clone(),
        Some(other) => panic!("expected IsDone, got {other:?}"),
        None => panic!("no IsDone chunk emitted"),
    };
    assert_eq!(result.text, "Looking up...");
    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].id, "call_w");
    assert_eq!(result.tool_calls[0].name, "get_weather");
    assert_eq!(
        result.tool_calls[0].input,
        serde_json::json!({"city": "Beijing"})
    );
}

/// Some OpenAI-compatible gateways send
/// `"tool_calls": []` as a heartbeat mid-stream between
/// function-call deltas (the wire shape is
/// indistinguishable from "no tools at all"). The
/// processor MUST treat this as a no-op (no chunks
/// emitted, no buffer mutation) — neither a panic nor
/// a spurious `IsDone`. Without this contract, a
/// benign heartbeat would crash the stream or close
/// it prematurely. Pins down the
/// `for tc in tool_calls` over an empty `Vec` loop
/// doing nothing.
#[test]
fn empty_tool_calls_array_is_a_no_op_not_a_termination_signal() {
    let mut p = OpenAIStreamProcessor::new();
    // Open a tool buffer first.
    let d1 = r#"{"id":"x","choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_x","type":"function","function":{"name":"get_weather","arguments":"{\"loc\":"}}]}}]}"#;
    p.process_line(d1);
    // Heartbeat with empty tool_calls array.
    let hb = r#"{"id":"x","choices":[{"delta":{"tool_calls":[]}}]}"#;
    let cs = p.process_line(hb);
    assert!(
        cs.is_empty(),
        "empty tool_calls array must be a no-op; got {cs:?}"
    );
    // The previously-opened tool buffer must still be
    // intact — a subsequent delta must still patch it.
    let d2 = r#"{"id":"x","choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ation\":\"Beijing\"}"}}]}}]}"#;
    let cs2 = p.process_line(d2);
    assert!(
        cs2.iter()
            .any(|c| matches!(c, StreamChunk::ToolCallDelta { .. })),
        "tool buffer must survive a tool_calls=[] heartbeat; got {cs2:?}"
    );
    // Close via finish_reason.
    let end = r#"{"id":"x","choices":[{"finish_reason":"tool_calls"}]}"#;
    let cs3 = p.process_line(end);
    let is_done = cs3
        .iter()
        .find_map(|c| match c {
            StreamChunk::IsDone { result } => Some((**result).clone()),
            _ => None,
        })
        .expect("IsDone must fire");
    assert_eq!(is_done.tool_calls.len(), 1);
    // The exact `input` value depends on whether the
    // accumulated raw string parses as JSON; the
    // canonical happy-path JSON args fixture
    // intentionally produces invalid JSON at this
    // delta split (the first delta only carries
    // `"{\"loc\":"`, not a complete object), so
    // `parse_tool_input` falls back to a `String`
    // wrapper. The point of THIS test is that the
    // buffer survived the `tool_calls:[]` heartbeat —
    // not the JSON parse result. We pin the buffer's
    // id and name (the deltas before/after the
    // heartbeat) and assert the buffer is non-empty.
    assert_eq!(is_done.tool_calls[0].id, "call_x");
    assert_eq!(is_done.tool_calls[0].name, "get_weather");
    assert!(
        !matches!(is_done.tool_calls[0].input, serde_json::Value::Null),
        "buffer must not be empty after surviving the heartbeat; got {:?}",
        is_done.tool_calls[0].input
    );
}

/// The first delta of an OpenAI turn typically carries
/// only `{"role": "assistant"}` with no `content` and
/// no `tool_calls`. This is the canonical "I'm starting
/// to speak" heartbeat. The processor MUST accept
/// this delta without crashing (the `role` field is
/// not modeled by `OpenAIDelta`, so serde silently
/// drops it as an unknown field), MUST NOT emit any
/// chunks, and MUST NOT terminate the stream. Pins
/// down the serde-default behavior for unknown
/// fields and the no-op handling of an
/// `OpenAIDelta` whose every modeled field is `None`.
#[test]
fn delta_with_only_role_field_is_silently_accepted() {
    let mut p = OpenAIStreamProcessor::new();
    let line = r#"{"id":"x","choices":[{"delta":{"role":"assistant"}}]}"#;
    let cs = p.process_line(line);
    assert!(
        cs.is_empty(),
        "delta with only `role` field must emit no chunks; got {cs:?}"
    );
    // Subsequent normal delta must work fine.
    let cs2 =
        p.process_line(r#"{"id":"x","choices":[{"delta":{"content":"hi"}}]}"#);
    assert_eq!(cs2.len(), 1);
    assert!(matches!(
        &cs2[0],
        StreamChunk::Content(ContentPart::Text(_))
    ));
    // Drive to IsDone and confirm text accumulated.
    let end = r#"{"id":"x","choices":[{"finish_reason":"stop"}]}"#;
    let cs_done = p.process_line(end);
    let result = match cs_done
        .iter()
        .find(|c| matches!(c, StreamChunk::IsDone { .. }))
    {
        Some(StreamChunk::IsDone { result }) => (**result).clone(),
        Some(other) => panic!("expected IsDone, got {other:?}"),
        None => panic!("no IsDone chunk emitted"),
    };
    assert_eq!(result.text, "hi");
}

/// When the upstream sends `n > 1` (multi-sample), each
/// SSE message carries an array of N choices. Our
/// processor is single-stream and processes only the
/// FIRST choice — the rest are silently dropped. This
/// pins the documented "n > 1 collapses to choice[0]"
/// contract. Without this contract, an upstream that
/// sends multi-sample would either: (a) crash on the
/// `choices.first()` assumption, or (b) silently mix
/// samples into one stream (which would corrupt the
/// agent's tool_calls array). Pins down that we
/// neither (a) nor (b) — we accept only choice[0].
#[test]
fn multi_choice_n_gt_1_takes_only_first_choice() {
    let mut p = OpenAIStreamProcessor::new();
    // Three choices, each with different content.
    let line = r#"{"id":"x","choices":[{"delta":{"content":"first "},"index":0},{"delta":{"content":"SECOND "},"index":1},{"delta":{"content":"third"},"index":2}]}"#;
    let cs = p.process_line(line);
    // Only "first " makes it into the stream — the
    // other two are silently dropped.
    assert_eq!(cs.len(), 1);
    match &cs[0] {
        StreamChunk::Content(ContentPart::Text(t)) => {
            assert_eq!(
                t.text, "first ",
                "only the first choice must be processed; got {:?}",
                t.text
            );
        }
        other => panic!("expected Content(Text), got {other:?}"),
    }
    // Drive to IsDone and verify text accumulated
    // only from the first choice.
    let end = r#"{"id":"x","choices":[{"finish_reason":"stop"}]}"#;
    let cs_done = p.process_line(end);
    let result = match cs_done
        .iter()
        .find(|c| matches!(c, StreamChunk::IsDone { .. }))
    {
        Some(StreamChunk::IsDone { result }) => (**result).clone(),
        Some(other) => panic!("expected IsDone, got {other:?}"),
        None => panic!("no IsDone chunk emitted"),
    };
    assert_eq!(
        result.text, "first ",
        "IsDone.text must contain only the first choice's text; got {:?}",
        result.text
    );
}

/// The SSE transport layer hands `process_line` a
/// stream of lines after splitting on `\n\n`. Some of
/// those lines are NOT OpenAI JSON payloads — they
/// include:
///   1. empty lines (SSE keepalive between events)
///   2. SSE comments like `: heartbeat` (the `:`
///      prefix marks a comment, not data)
///   3. whitespace-only padding
///   4. truly malformed JSON (truncation, network
///      corruption, malformed upstream)
///
/// The processor MUST treat ALL of these as silent
/// no-ops — neither panic, emit chunks, nor terminate
/// the stream. Without this contract, a single benign
/// SSE keepalive would crash the stream and lose all
/// subsequent tool_calls. Pins the
/// `let Ok(delta_resp) = ... else { return vec![]; }`
/// defensive path at line 62.
#[test]
fn non_json_lines_are_silently_dropped_without_terminating() {
    let mut p = OpenAIStreamProcessor::new();
    // 1. Empty line.
    assert!(p.process_line("").is_empty());
    // 2. Whitespace-only.
    assert!(p.process_line("   \t  ").is_empty());
    // 3. SSE comment (`: heartbeat`).
    assert!(p.process_line(": heartbeat").is_empty());
    // 4. Truly malformed JSON.
    assert!(p.process_line("{not json").is_empty());
    assert!(p.process_line("data: {choices}").is_empty());
    // The processor must NOT have transitioned to
    // finished — a real delta after the noise must
    // still be processed.
    let cs = p.process_line(
        r#"{"id":"x","choices":[{"delta":{"content":"hello"}}]}"#,
    );
    assert_eq!(cs.len(), 1);
    assert!(matches!(&cs[0], StreamChunk::Content(ContentPart::Text(_))));
    // Drive to IsDone.
    let cs_done = p.process_line("[DONE]");
    let result = cs_done
        .iter()
        .find_map(|c| match c {
            StreamChunk::IsDone { result } => Some((**result).clone()),
            _ => None,
        })
        .expect("IsDone must fire on [DONE]");
    assert_eq!(result.text, "hello");
}

/// A SINGLE choice may carry both `delta.content`
/// AND `finish_reason: null` mid-stream — this is
/// what OpenAI sends between the last token delta
/// and the trailing usage-only choice when
/// `stream_options.include_usage` is set. The
/// processor MUST emit the text chunk (so the
/// frontend sees the token live) AND MUST NOT emit
/// IsDone (because `finish_reason` is null, the
/// stream is not yet done — a follow-up usage choice
/// with `finish_reason: "stop"` is coming). Pins the
/// `if let Some(ref finish_reason) = ...` guard at
/// line 196.
#[test]
fn midstream_delta_with_finish_reason_null_emits_text_but_not_is_done() {
    let mut p = OpenAIStreamProcessor::new();
    // Mid-stream delta: content present, finish_reason
    // explicitly null, no usage yet.
    let mid = r#"{"id":"x","choices":[{"delta":{"content":"hello"},"finish_reason":null}]}"#;
    let cs = p.process_line(mid);
    // MUST emit text chunk.
    assert_eq!(cs.len(), 1);
    match &cs[0] {
        StreamChunk::Content(ContentPart::Text(t)) => {
            assert_eq!(t.text, "hello");
        }
        other => panic!("expected Content(Text), got {other:?}"),
    }
    // MUST NOT emit IsDone — `finish_reason: null`
    // means the stream is not terminated.
    assert!(
        !cs.iter().any(|c| matches!(c, StreamChunk::IsDone { .. })),
        "mid-stream delta with finish_reason=null must NOT emit IsDone; got {cs:?}"
    );
    // Follow-up usage chunk arrives (with finish_reason
    // still null). Usage capture must succeed, IsDone
    // still NOT emitted.
    let usage = r#"{"id":"x","choices":[{"delta":{},"finish_reason":null,"usage":{"prompt_tokens":5,"completion_tokens":1,"total_tokens":6}}]}"#;
    let cs_u = p.process_line(usage);
    assert!(
        cs_u.iter().any(|c| matches!(c, StreamChunk::Usage(_))),
        "usage chunk must still be emitted mid-stream; got {cs_u:?}"
    );
    assert!(
        !cs_u.iter().any(|c| matches!(c, StreamChunk::IsDone { .. })),
        "usage chunk with finish_reason=null must NOT emit IsDone; got {cs_u:?}"
    );
    // Final terminate.
    let cs_done = p.process_line("[DONE]");
    let result = cs_done
        .iter()
        .find_map(|c| match c {
            StreamChunk::IsDone { result } => Some((**result).clone()),
            _ => None,
        })
        .expect("IsDone must fire on [DONE]");
    assert_eq!(result.text, "hello");
    assert_eq!(result.usage.total_tokens, 6);
}
