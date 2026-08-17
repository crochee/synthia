use std::collections::HashMap;

use super::types::OpenAIDeltaResponse;
use crate::{
    streaming::{ThinkExtractor, ToolUseBuffer, parse_tool_input},
    types::{
        ContentPart,
        ReasoningContent,
        SamplingResult,
        StreamChunk,
        TokenUsage,
        ToolUse,
    },
};

pub struct OpenAIStreamProcessor {
    tool_buffers: HashMap<usize, ToolUseBuffer>,
    /// Per-turn accumulators for the final `SamplingResult`.
    text: String,
    reasoning: String,
    tool_calls: Vec<ToolUse>,
    usage: Option<TokenUsage>,
    /// Canonical stop reason captured from the last
    /// `finish_reason` chunk in this turn. Propagated to
    /// the final `SamplingResult.stop_reason` so the
    /// agent layer can distinguish `stop`, `tool_calls`,
    /// `length`, etc. without parsing the SSE itself.
    stop_reason: Option<String>,
    /// `true` after `IsDone` has been emitted; further events are
    /// ignored.
    finished: bool,
    /// Per-turn extractor that splits `<think>…</think>` markers out
    /// of plain-text deltas. Providers like MiniMax-M2.7 emit reasoning
    /// inline in `delta.content`; without this extractor the reasoning
    /// ends up concatenated with the visible answer on the frontend.
    think: ThinkExtractor,
}

impl OpenAIStreamProcessor {
    pub fn new() -> Self {
        Self {
            tool_buffers: HashMap::with_capacity(4),
            text: String::new(),
            reasoning: String::new(),
            tool_calls: Vec::new(),
            usage: None,
            stop_reason: None,
            finished: false,
            think: ThinkExtractor::new(),
        }
    }

    pub fn process_line(&mut self, data: &str) -> Vec<StreamChunk> {
        if self.finished {
            return vec![];
        }
        let data = data.trim();
        if data == "[DONE]" {
            return vec![self.emit_is_done(None)];
        }

        let Ok(delta_resp) = serde_json::from_str::<OpenAIDeltaResponse>(data)
        else {
            return vec![];
        };

        // OpenAI streaming: when a request is sent with `n > 1`,
        // each SSE message carries an array of N choices (one per
        // sample). Our agent layer consumes one stream per turn, so
        // we process only the FIRST choice and silently drop the
        // rest. Clients that need multi-sample parallel streams
        // should send `n = 1` per request. Any caller sending `n > 1`
        // gets a single canonical stream — the dropped choices are
        // not recoverable from this processor.
        let Some(choice) = delta_resp.choices.first() else {
            return vec![];
        };

        let mut chunks = Vec::with_capacity(8);

        // Reasoning is emitted as a dedicated Content(Reasoning) chunk
        // without any text sniffing — the upstream already separates
        // reasoning from text into its own field.
        if let Some(ref delta) = choice.delta {
            if let Some(ref reasoning) = delta.reasoning_content
                && !reasoning.is_empty()
            {
                self.reasoning.push_str(reasoning);
                chunks.push(StreamChunk::Content(ContentPart::Reasoning(
                    ReasoningContent {
                        text: reasoning.clone(),
                        signature: None,
                    },
                )));
            }
            if let Some(ref content) = delta.content
                && !content.is_empty()
            {
                // Route text deltas through the shared ThinkExtractor
                // so `<think>…</think>` markers get split into separate
                // reasoning chunks. The extractor also folds the safe
                // (non-marker) prefix into `self.text` so the final
                // `SamplingResult` matches what the streaming chunks
                // emitted.
                let mut extracted = self.think.process_text(content);
                for chunk in &extracted {
                    if let StreamChunk::Content(ContentPart::Text(t)) = chunk {
                        self.text.push_str(&t.text);
                    } else if let StreamChunk::Content(
                        ContentPart::Reasoning(r),
                    ) = chunk
                    {
                        self.reasoning.push_str(&r.text);
                    }
                }
                chunks.append(&mut extracted);
            }
            if let Some(ref tool_calls) = delta.tool_calls {
                for tc in tool_calls {
                    let index = tc.index.unwrap_or(0) as usize;
                    let tc_id = tc
                        .id
                        .clone()
                        .unwrap_or_else(|| format!("call_{index}"));

                    // First time we see this index: emit ToolCallStart.
                    match self.tool_buffers.entry(index) {
                        std::collections::hash_map::Entry::Vacant(vacant) => {
                            let name = tc
                                .function
                                .name
                                .clone()
                                .unwrap_or_else(|| "unknown".to_string());
                            // The first delta may already include a partial
                            // argument string; treat that as part of the
                            // input buffer and emit only the start chunk.
                            let initial = tc
                                .function
                                .arguments
                                .clone()
                                .unwrap_or_default();
                            chunks.push(StreamChunk::ToolCallStart {
                                id: tc_id.clone(),
                                name: name.clone(),
                                arguments: serde_json::Value::String(
                                    initial.clone(),
                                ),
                            });
                            vacant.insert(ToolUseBuffer {
                                id: tc_id,
                                name,
                                input: initial,
                            });
                        }
                        std::collections::hash_map::Entry::Occupied(
                            mut occ,
                        ) => {
                            // Subsequent delta: patch name (rare) and
                            // append argument delta.
                            if let Some(ref name) = tc.function.name {
                                occ.get_mut().name = name.clone();
                            }
                            if let Some(args) = tc.function.arguments.as_ref()
                                && !args.is_empty()
                            {
                                let buffer = occ.get_mut();
                                buffer.input.push_str(args);
                                chunks.push(StreamChunk::ToolCallDelta {
                                    id: buffer.id.clone(),
                                    arguments_delta: args.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }

        // Some providers (e.g. OpenAI with `stream_options.include_usage`)
        // attach a usage block to a final choice with empty delta and
        // `finish_reason: null`. Capture it without emitting IsDone yet.
        if let Some(usage) = &choice.usage {
            let u = TokenUsage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
                cached_prompt_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
            };
            self.usage = Some(u.clone());
            chunks.push(StreamChunk::Usage(u));
        }

        if let Some(ref finish_reason) = choice.finish_reason {
            let reason = finish_reason.clone();
            // Flush all open tool calls (this is the OpenAI-style "end
            // of stream" for tool calls).
            for (_, buffer) in
                std::mem::take(&mut self.tool_buffers).into_iter()
            {
                let parsed = parse_tool_input(&buffer.input);
                let tool_use = ToolUse {
                    id: buffer.id.clone(),
                    name: buffer.name.clone(),
                    input: parsed,
                };
                self.tool_calls.push(tool_use);
                chunks.push(StreamChunk::ToolCallEnd {
                    id: buffer.id.clone(),
                });
            }
            let done = self.emit_is_done(Some(reason));
            chunks.push(done);
        }

        chunks
    }

    fn emit_is_done(&mut self, finish_reason: Option<String>) -> StreamChunk {
        self.finished = true;
        // Capture the canonical stop reason so it
        // propagates into the final `SamplingResult`.
        // When the stream terminates via `[DONE]` without
        // a preceding `finish_reason` (malformed
        // upstream), the stop reason is unknown — leave
        // it as `None` so consumers can distinguish
        // "missing" from "explicit stop".
        if let Some(ref reason) = finish_reason {
            self.stop_reason = Some(reason.clone());
        }
        // If `finish_reason` was Some, the `process_line`
        // path already drained `tool_buffers` and pushed
        // `ToolCallEnd` chunks BEFORE calling `emit_is_done`.
        // But if the stream ended via `[DONE]` with no
        // preceding `finish_reason` (malformed upstream,
        // truncation), `tool_buffers` may still hold live
        // entries that the caller will never see emitted.
        // Flush them here so the final `SamplingResult`
        // is consistent with the buffered calls: every
        // `ToolUse` in `tool_calls` was at least seen by
        // the processor (whether it was fully closed is a
        // separate question).
        //
        // We do NOT synthesize `ToolCallEnd` chunks here
        // because downstream consumers already key off
        // the `IsDone.tool_calls` vec; emitting `ToolCallEnd`
        // from this fallback path would cause double-emit
        // for the normal `finish_reason`-driven path. The
        // drain-then-aggregate is the same approach used
        // by the anthropic processor (see
        // `streaming/anthropic/processor.rs::message_stop`).
        if finish_reason.is_none() {
            for (_, buffer) in
                std::mem::take(&mut self.tool_buffers).into_iter()
            {
                let parsed = parse_tool_input(&buffer.input);
                self.tool_calls.push(ToolUse {
                    id: buffer.id,
                    name: buffer.name,
                    input: parsed,
                });
            }
        }
        let usage = self.usage.clone().unwrap_or_default();
        // Drain any residual text or reasoning the ThinkExtractor
        // was still holding as carry. These would otherwise be lost
        // (the carry is only released when the next delta arrives,
        // but the next delta after this point is `[DONE]` or a
        // terminal `finish_reason` chunk with empty content).
        let mut drained = self.think.flush();
        for chunk in &drained {
            if let StreamChunk::Content(ContentPart::Text(t)) = chunk {
                self.text.push_str(&t.text);
            } else if let StreamChunk::Content(ContentPart::Reasoning(r)) =
                chunk
            {
                self.reasoning.push_str(&r.text);
            }
        }
        drained.clear();
        let text = std::mem::take(&mut self.text);
        let reasoning = std::mem::take(&mut self.reasoning);
        let tool_calls = std::mem::take(&mut self.tool_calls);
        let result = SamplingResult {
            text,
            tool_calls,
            reasoning,
            reasoning_signature: None,
            usage,
            stop_reason: std::mem::take(&mut self.stop_reason),
        };
        tracing::debug!(
            target: "synthia_provider::openai::processor",
            text_len = result.text.len(),
            tool_calls = result.tool_calls.len(),
            "openai stream complete"
        );
        StreamChunk::IsDone {
            result: Box::new(result),
        }
    }
}

impl Default for OpenAIStreamProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- new() / default() -------------------------------------------

    /// `OpenAIStreamProcessor::new()` MUST produce a processor
    /// that ignores input until `process_line` is called.
    #[test]
    fn new_constructs_empty_processor() {
        let mut p = OpenAIStreamProcessor::new();
        let chunks = p.process_line("");
        assert!(chunks.is_empty());
    }

    /// `OpenAIStreamProcessor::default()` MUST match `new()`.
    #[test]
    fn default_matches_new() {
        let mut p = OpenAIStreamProcessor::default();
        // Empty input → empty chunks.
        assert!(p.process_line("").is_empty());
    }

    // -- [DONE] sentinel ---------------------------------------------

    /// `process_line("[DONE]")` MUST emit a single `IsDone`
    /// chunk with no stop reason (no `finish_reason` was seen).
    #[test]
    fn done_sentinel_emits_is_done_with_no_stop_reason() {
        let mut p = OpenAIStreamProcessor::new();
        let chunks = p.process_line("[DONE]");
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::IsDone { result } => {
                assert_eq!(result.stop_reason, None);
            }
            _ => panic!("expected IsDone"),
        }
    }

    /// After `IsDone` is emitted, further `process_line` calls
    /// MUST silently return empty (the `finished` flag).
    #[test]
    fn events_after_is_done_are_ignored() {
        let mut p = OpenAIStreamProcessor::new();
        p.process_line("[DONE]");
        // Any subsequent line MUST be a no-op.
        let chunks =
            p.process_line(r#"{"choices":[{"delta":{"content":"x"}}]}"#);
        assert!(chunks.is_empty());
    }

    // -- malformed JSON ----------------------------------------------

    /// Malformed JSON MUST silently produce no chunks (no panic,
    /// no error propagation).
    #[test]
    fn malformed_json_silently_ignored() {
        let mut p = OpenAIStreamProcessor::new();
        let chunks = p.process_line("{bad json");
        assert!(chunks.is_empty());
    }

    /// Empty input MUST produce no chunks (not [DONE], not
    /// an error).
    #[test]
    fn empty_input_produces_no_chunks() {
        let mut p = OpenAIStreamProcessor::new();
        let chunks = p.process_line("");
        assert!(chunks.is_empty());
    }

    /// Whitespace-only input MUST be trimmed and treated as
    /// empty (the `data.trim()` call).
    #[test]
    fn whitespace_only_input_produces_no_chunks() {
        let mut p = OpenAIStreamProcessor::new();
        let chunks = p.process_line("   \n  \t ");
        assert!(chunks.is_empty());
    }

    // -- content delta ------------------------------------------------

    /// A `delta.content` chunk MUST emit `Content(Text)` and
    /// accumulate text.
    #[test]
    fn content_delta_emits_text_chunk() {
        let mut p = OpenAIStreamProcessor::new();
        let chunks = p.process_line(
            r#"{"choices":[{"delta":{"content":"hello"},"finish_reason":null}]}"#,
        );
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::Content(ContentPart::Text(t)) => {
                assert_eq!(t.text, "hello");
            }
            other => panic!("expected Content(Text), got {other:?}"),
        }
    }

    /// Empty `delta.content` MUST NOT emit a chunk (the
    /// `!content.is_empty()` guard).
    #[test]
    fn empty_content_delta_emits_no_chunk() {
        let mut p = OpenAIStreamProcessor::new();
        let chunks = p.process_line(
            r#"{"choices":[{"delta":{"content":""},"finish_reason":null}]}"#,
        );
        assert!(chunks.is_empty());
    }

    /// Multiple `delta.content` chunks MUST accumulate into the
    /// final `SamplingResult.text`.
    #[test]
    fn content_accumulates_across_lines() {
        let mut p = OpenAIStreamProcessor::new();
        for chunk in ["hello", " ", "world"] {
            let json = format!(
                r#"{{"choices":[{{"delta":{{"content":"{chunk}"}},"finish_reason":null}}]}}"#,
            );
            p.process_line(&json);
        }
        let chunks = p.process_line("[DONE]");
        match &chunks[0] {
            StreamChunk::IsDone { result } => {
                assert_eq!(result.text, "hello world");
            }
            _ => panic!("expected IsDone"),
        }
    }

    // -- reasoning_content delta -------------------------------------

    /// `delta.reasoning_content` MUST emit `Content(Reasoning)`
    /// chunks.
    #[test]
    fn reasoning_content_delta_emits_reasoning_chunk() {
        let mut p = OpenAIStreamProcessor::new();
        let chunks = p.process_line(
            r#"{"choices":[{"delta":{"reasoning_content":"thinking..."},"finish_reason":null}]}"#,
        );
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::Content(ContentPart::Reasoning(r)) => {
                assert_eq!(r.text, "thinking...");
                assert_eq!(r.signature, None);
            }
            other => panic!("expected Content(Reasoning), got {other:?}"),
        }
    }

    /// `reasoning_content` and `content` in the same chunk
    /// MUST emit BOTH chunks (separately).
    #[test]
    fn reasoning_and_content_in_same_chunk() {
        let mut p = OpenAIStreamProcessor::new();
        let chunks = p.process_line(
            r#"{"choices":[{"delta":{"reasoning_content":"think","content":"answer"},"finish_reason":null}]}"#,
        );
        assert_eq!(chunks.len(), 2);
        let reasoning_chunk = chunks.iter().find(|c| {
            matches!(c, StreamChunk::Content(ContentPart::Reasoning(_)))
        });
        let text_chunk = chunks
            .iter()
            .find(|c| matches!(c, StreamChunk::Content(ContentPart::Text(_))));
        assert!(reasoning_chunk.is_some(), "no reasoning chunk");
        assert!(text_chunk.is_some(), "no text chunk");
    }

    // -- tool_calls delta ---------------------------------------------

    /// First `delta.tool_calls` MUST emit `ToolCallStart`.
    #[test]
    fn tool_call_first_delta_emits_tool_call_start() {
        let mut p = OpenAIStreamProcessor::new();
        let chunks = p.process_line(
            r#"{
                "choices":[{
                    "delta":{"tool_calls":[{
                        "id":"call-1",
                        "function":{"name":"bash","arguments":"{\"cmd\":\"ls\"}"},
                        "index":0
                    }]},
                    "finish_reason":null
                }]
            }"#,
        );
        // 1 ToolCallStart, no other chunks.
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::ToolCallStart {
                id,
                name,
                arguments,
            } => {
                assert_eq!(id, "call-1");
                assert_eq!(name, "bash");
                assert_eq!(
                    *arguments,
                    serde_json::json!("{\"cmd\":\"ls\"}".to_string())
                );
            }
            other => panic!("expected ToolCallStart, got {other:?}"),
        }
    }

    /// Subsequent `delta.tool_calls` deltas (with the same
    /// index) MUST emit `ToolCallDelta`, NOT a second
    /// `ToolCallStart`.
    #[test]
    fn tool_call_subsequent_delta_emits_tool_call_delta() {
        let mut p = OpenAIStreamProcessor::new();
        // First delta: start.
        p.process_line(
            r#"{
                "choices":[{
                    "delta":{"tool_calls":[{
                        "id":"call-1",
                        "function":{"name":"bash","arguments":""},
                        "index":0
                    }]},
                    "finish_reason":null
                }]
            }"#,
        );
        // Second delta: continuation.
        let chunks = p.process_line(
            r#"{
                "choices":[{
                    "delta":{"tool_calls":[{
                        "id":null,
                        "function":{"arguments":"{\"cmd\":\"l"},
                        "index":0
                    }]},
                    "finish_reason":null
                }]
            }"#,
        );
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::ToolCallDelta {
                id,
                arguments_delta,
            } => {
                assert_eq!(id, "call-1");
                assert_eq!(arguments_delta, "{\"cmd\":\"l");
            }
            other => panic!("expected ToolCallDelta, got {other:?}"),
        }
    }

    // -- usage block --------------------------------------------------

    /// A final choice with `usage` MUST emit a `Usage` chunk.
    #[test]
    fn final_usage_chunk_emits_usage() {
        let mut p = OpenAIStreamProcessor::new();
        let chunks = p.process_line(
            r#"{
                "choices":[{"delta":{},"finish_reason":null,"usage":{
                    "prompt_tokens":100,
                    "completion_tokens":50,
                    "total_tokens":150
                }}]
            }"#,
        );
        // 1 Usage chunk (no IsDone — finish_reason was null).
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::Usage(u) => {
                assert_eq!(u.prompt_tokens, 100);
                assert_eq!(u.completion_tokens, 50);
                assert_eq!(u.total_tokens, 150);
            }
            other => panic!("expected Usage, got {other:?}"),
        }
    }

    // -- finish_reason ------------------------------------------------

    /// `finish_reason` MUST emit `ToolCallEnd` chunks for each
    /// buffered tool call, then `IsDone` with the reason.
    #[test]
    fn finish_reason_drains_tool_calls_and_emits_is_done() {
        let mut p = OpenAIStreamProcessor::new();
        // First start a tool call (no finish_reason yet).
        p.process_line(
            r#"{
                "choices":[{
                    "delta":{"tool_calls":[{
                        "id":"call-1",
                        "function":{"name":"bash","arguments":"{\"cmd\":\"ls\"}"},
                        "index":0
                    }]},
                    "finish_reason":null
                }]
            }"#,
        );
        // Now send finish_reason.
        let chunks = p.process_line(
            r#"{
                "choices":[{"delta":{},"finish_reason":"tool_calls"}]
            }"#,
        );
        // ToolCallEnd + IsDone.
        assert_eq!(chunks.len(), 2);
        assert!(
            matches!(&chunks[0], StreamChunk::ToolCallEnd { id } if id == "call-1")
        );
        match &chunks[1] {
            StreamChunk::IsDone { result } => {
                assert_eq!(result.stop_reason, Some("tool_calls".to_string()));
                assert_eq!(result.tool_calls.len(), 1);
                assert_eq!(result.tool_calls[0].name, "bash");
            }
            _ => panic!("expected IsDone as second chunk"),
        }
    }

    // -- multi-choice handling ----------------------------------------

    /// Multi-choice response MUST process only the FIRST
    /// choice and silently drop the rest.
    #[test]
    fn multi_choice_processes_only_first() {
        let mut p = OpenAIStreamProcessor::new();
        let chunks = p.process_line(
            r#"{
                "choices":[
                    {"delta":{"content":"first"},"finish_reason":null},
                    {"delta":{"content":"second"},"finish_reason":null}
                ]
            }"#,
        );
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::Content(ContentPart::Text(t)) => {
                assert_eq!(t.text, "first");
            }
            _ => panic!("expected Content(Text)"),
        }
    }

    /// Empty `choices` array MUST produce no chunks (no panic).
    #[test]
    fn empty_choices_array_produces_no_chunks() {
        let mut p = OpenAIStreamProcessor::new();
        let chunks = p.process_line(r#"{"choices":[]}"#);
        assert!(chunks.is_empty());
    }

    // -- orphan tool buffers on [DONE] -------------------------------

    /// When `[DONE]` arrives with live `tool_buffers`
    /// (truncated stream), the `IsDone.tool_calls` MUST still
    /// include them (silent drain, no `ToolCallEnd` chunks).
    #[test]
    fn done_sentinel_drains_orphan_tool_buffers() {
        let mut p = OpenAIStreamProcessor::new();
        // Start a tool call but never close it.
        p.process_line(
            r#"{
                "choices":[{
                    "delta":{"tool_calls":[{
                        "id":"call-orphan",
                        "function":{"name":"bash","arguments":"{\"cmd\":\"l"},
                        "index":0
                    }]},
                    "finish_reason":null
                }]
            }"#,
        );
        // Stream ends via [DONE] (no finish_reason).
        let chunks = p.process_line("[DONE]");
        match &chunks[0] {
            StreamChunk::IsDone { result } => {
                // The orphan was drained into tool_calls even
                // though no ToolCallEnd was emitted.
                assert_eq!(result.tool_calls.len(), 1);
                assert_eq!(result.tool_calls[0].id, "call-orphan");
            }
            _ => panic!("expected IsDone"),
        }
    }
}
