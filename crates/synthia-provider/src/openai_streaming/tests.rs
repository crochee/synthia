use serde_json::json;

use super::*;
use crate::types::{ContentPart, ReasoningContent, StreamChunk};

#[test]
fn content_delta_emits_text_chunk() {
    let mut p = OpenAIStreamProcessorV2::new();
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
    let mut p = OpenAIStreamProcessorV2::new();
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

#[test]
fn tool_call_emits_start_delta_end_with_is_done() {
    let mut p = OpenAIStreamProcessorV2::new();
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
    let v = super::processor::parse_tool_input("");
    assert!(v.is_object());
    assert!(v.as_object().unwrap().is_empty());
}

#[test]
fn parse_tool_input_handles_invalid_json() {
    let v = super::processor::parse_tool_input("not json");
    assert_eq!(v, json!("not json"));
}

#[test]
fn done_token_emits_is_done() {
    let mut p = OpenAIStreamProcessorV2::new();
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

/// Task 1.7: OpenAI streaming produces reasoning chunks without a
/// signature (OpenAI doesn't emit one). The reasoning part and the
/// final `SamplingResult` must both leave `signature` / `reasoning_signature`
/// as `None`.
#[test]
fn openai_reasoning_chunks_leave_signature_none() {
    let mut p = OpenAIStreamProcessorV2::new();
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
