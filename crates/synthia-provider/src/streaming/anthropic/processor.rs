//! The [`StreamProcessor`] used by `complete_with_stream`.
//! Emits incremental
//! `ToolCallStart { id, name, arguments }`,
//! `ToolCallDelta { id, arguments_delta }`, `ToolCallEnd { id }`,
//! and a terminal `IsDone { result: SamplingResult }` on
//! `message_stop`.

use std::collections::HashMap;

use tracing::warn;

use super::events::AnthropicStreamEvent;
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

/// Stream processor that emits the chunk model required by
/// `complete_with_stream`:
/// - `Content(Text)` for `text_delta`
/// - `Content(Reasoning)` for `thinking_delta`
/// - `ToolCallStart { id, name, arguments }` on `content_block_start` (tool_use)
/// - `ToolCallDelta { id, arguments_delta }` on `input_json_delta`
/// - `ToolCallEnd { id }` on `content_block_stop`
/// - `Usage(...)` and `IsDone { result }` on `message_delta` / `message_stop`
pub struct StreamProcessor {
    tool_buffers: HashMap<usize, ToolUseBuffer>,
    /// Per-turn accumulators for the final `SamplingResult`.
    text: String,
    reasoning: String,
    /// Latest Anthropic `signature_delta` observed during this turn
    /// (if any). Folded into the final `SamplingResult.reasoning_signature`
    /// so the agent layer can preserve reasoning continuity across turns.
    last_reasoning_signature: Option<String>,
    tool_calls: Vec<ToolUse>,
    /// Usage is optional; Anthropic emits it on a `message_delta` event.
    usage: Option<TokenUsage>,
    /// Stop reason captured from `message_delta.stop_reason` or
    /// `message_stop`.
    stop_reason: Option<String>,
    /// Per-turn extractor that splits `<think>…</think>` markers out
    /// of `text_delta` deltas. Anthropic's native `thinking_delta`
    /// events are routed through `ContentPart::Reasoning` directly,
    /// but a few upstreams (notably MiniMax-M2.7 reached via the
    /// Anthropic-compatible endpoint) embed reasoning inline in the
    /// text block; this extractor handles that case so the frontend
    /// still sees a clean separation.
    think: ThinkExtractor,
}

impl StreamProcessor {
    pub fn new() -> Self {
        Self {
            tool_buffers: HashMap::with_capacity(4),
            text: String::new(),
            reasoning: String::new(),
            last_reasoning_signature: None,
            tool_calls: Vec::new(),
            usage: None,
            stop_reason: None,
            think: ThinkExtractor::new(),
        }
    }

    pub fn process_event(
        &mut self,
        event: &AnthropicStreamEvent,
    ) -> Vec<StreamChunk> {
        let mut chunks = Vec::with_capacity(8);

        match event.r#type.as_str() {
            "content_block_start" => {
                let (Some(block), Some(index)) =
                    (&event.content_block, event.index)
                else {
                    return chunks;
                };

                match block.r#type.as_str() {
                    "tool_use" => {
                        let (Some(id), Some(name)) = (&block.id, &block.name)
                        else {
                            return chunks;
                        };
                        let input_str = block.input.clone().unwrap_or_default();
                        self.tool_buffers.insert(
                            index,
                            ToolUseBuffer {
                                id: id.clone(),
                                name: name.clone(),
                                input: input_str.clone(),
                            },
                        );
                        chunks.push(StreamChunk::ToolCallStart {
                            id: id.clone(),
                            name: name.clone(),
                            arguments: serde_json::Value::String(input_str),
                        });
                    }
                    "thinking" => {
                        if let Some(thinking) = &block.thinking {
                            self.reasoning.push_str(thinking);
                            chunks.push(StreamChunk::Content(
                                ContentPart::Reasoning(ReasoningContent {
                                    text: thinking.clone(),
                                    signature: self
                                        .last_reasoning_signature
                                        .clone(),
                                }),
                            ));
                        }
                    }
                    "redacted_thinking" => {
                        let marker = "[Redacted by safety filter]".to_string();
                        self.reasoning.push_str(&marker);
                        chunks.push(StreamChunk::Content(
                            ContentPart::Reasoning(ReasoningContent {
                                text: marker,
                                signature: self
                                    .last_reasoning_signature
                                    .clone(),
                            }),
                        ));
                    }
                    "server_tool_use" => {
                        warn!("server_tool_use not supported");
                    }
                    _ => {}
                }
            }
            "content_block_delta" => {
                let (Some(delta), Some(index)) = (&event.delta, event.index)
                else {
                    return chunks;
                };

                match delta.r#type.as_str() {
                    "text_delta" => {
                        if let Some(text) = &delta.text {
                            // Route text deltas through the shared
                            // ThinkExtractor so `<think>…</think>`
                            // markers are split into separate
                            // reasoning chunks. Anthropic-native
                            // thinking blocks already arrive as
                            // `thinking_delta` events and bypass
                            // this path; this handles upstreams
                            // (e.g. MiniMax-M2.7) that embed
                            // reasoning inside the text block.
                            let mut extracted = self.think.process_text(text);
                            for chunk in &extracted {
                                if let StreamChunk::Content(
                                    ContentPart::Text(t),
                                ) = chunk
                                {
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
                    }
                    "thinking_delta" => {
                        if let Some(thinking) = &delta.thinking {
                            self.reasoning.push_str(thinking);
                            chunks.push(StreamChunk::Content(
                                ContentPart::Reasoning(ReasoningContent {
                                    text: thinking.clone(),
                                    signature: self
                                        .last_reasoning_signature
                                        .clone(),
                                }),
                            ));
                        }
                    }
                    "signature_delta" => {
                        // The signature folds into the most recent
                        // reasoning block on finalize; we do not emit a
                        // stream chunk here because the agent will
                        // read the signature off `SamplingResult`.
                        if let Some(signature) = &delta.signature {
                            self.last_reasoning_signature =
                                Some(signature.clone());
                        }
                    }
                    "input_json_delta" => {
                        if let (Some(partial), Some(buffer)) = (
                            delta.partial_json.as_ref(),
                            self.tool_buffers.get_mut(&index),
                        ) {
                            buffer.input.push_str(partial);
                            chunks.push(StreamChunk::ToolCallDelta {
                                id: buffer.id.clone(),
                                arguments_delta: partial.clone(),
                            });
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                if let Some(index) = event.index
                    && let Some(buffer) = self.tool_buffers.remove(&index)
                {
                    // Parse the accumulated JSON once, here, so callers
                    // don't have to re-parse the raw string at the end.
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
            }
            "message_delta" => {
                // Note: `message_delta` carries the turn-level
                // `usage` block, but `AnthropicStreamDelta` (the
                // streaming delta struct) doesn't model it. Usage
                // tracking is deferred to the PR2 provider-level
                // work; for now only the stop reason is propagated.
                if let Some(reason) = &event.stop_reason {
                    self.stop_reason = Some(reason.clone());
                }
            }
            "message_stop" => {
                if event.stop_reason.is_some() {
                    self.stop_reason = event.stop_reason.clone();
                }
                // If any `tool_use` content blocks were
                // started but never closed (no matching
                // `content_block_stop`), the per-index
                // buffers are still live. Anthropic's API
                // guarantees a `content_block_stop` for
                // every `content_block_start`, so a
                // non-empty buffer at `message_stop` means
                // the stream was truncated mid-tool-use
                // (network drop, malformed SSE, etc.). Log
                // a warning so the operator can correlate
                // missing tool calls back to the stream
                // gap, but do NOT synthesize a `ToolUse`
                // — the input JSON is by definition
                // incomplete and would either fail the
                // downstream tool's schema or execute
                // with empty args. The downstream tool
                // dispatcher will simply see no
                // `tool_calls` in the final `IsDone`
                // payload, which is the correct signal.
                if !self.tool_buffers.is_empty() {
                    let orphaned: Vec<String> = self
                        .tool_buffers
                        .values()
                        .map(|b| format!("{}:{}", b.id, b.name))
                        .collect();
                    warn!(
                        target: "synthia.provider",
                        count = orphaned.len(),
                        tools = ?orphaned,
                        "anthropic stream ended with orphan tool_use buffers; \
                         the matching content_block_stop events were missing \
                         (truncated SSE, network drop, or malformed upstream)"
                    );
                }
                // Drain any residual text or reasoning the
                // ThinkExtractor was still holding as carry so
                // the aggregated `SamplingResult` matches the
                // streaming chunks.
                let mut drained = self.think.flush();
                for chunk in &drained {
                    if let StreamChunk::Content(ContentPart::Text(t)) = chunk {
                        self.text.push_str(&t.text);
                    } else if let StreamChunk::Content(
                        ContentPart::Reasoning(r),
                    ) = chunk
                    {
                        self.reasoning.push_str(&r.text);
                    }
                }
                drained.clear();
                let usage = self.usage.clone().unwrap_or_default();
                let stop = self
                    .stop_reason
                    .clone()
                    .unwrap_or_else(|| "end_turn".to_string());
                let reasoning = std::mem::take(&mut self.reasoning);
                let text = std::mem::take(&mut self.text);
                let tool_calls = std::mem::take(&mut self.tool_calls);
                let reasoning_signature =
                    std::mem::take(&mut self.last_reasoning_signature);
                let result = SamplingResult {
                    text,
                    tool_calls,
                    reasoning,
                    reasoning_signature,
                    usage,
                    stop_reason: Some(stop.clone()),
                };
                tracing::debug!(
                    target: "synthia_provider::anthropic::stream",
                    stop_reason = %stop,
                    text_len = result.text.len(),
                    tool_calls = result.tool_calls.len(),
                    "anthropic stream complete"
                );
                chunks.push(StreamChunk::IsDone {
                    result: Box::new(result),
                });
            }
            _ => {}
        }

        chunks
    }
}

impl Default for StreamProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::streaming::anthropic::events::{
        AnthropicStreamContentBlock,
        AnthropicStreamDelta,
        AnthropicStreamEvent,
    };

    // -- new() / default() -------------------------------------------

    /// `StreamProcessor::new()` MUST initialize all fields to
    /// empty/sensible defaults.
    #[test]
    fn new_initializes_all_fields() {
        let mut p = StreamProcessor::new();
        // No public getters, but we exercise the
        // default-state by sending a no-op event.
        let ev = AnthropicStreamEvent {
            r#type: "ping".to_string(),
            content_block: None,
            index: None,
            delta: None,
            stop_reason: None,
        };
        let chunks = p.process_event(&ev);
        assert!(chunks.is_empty(), "ping event produces no chunks");
    }

    /// `StreamProcessor::default()` MUST match `new()`.
    #[test]
    fn default_matches_new() {
        // Both produce processors that return empty chunks
        // for unknown events.
        let mut p = StreamProcessor::default();
        let ev = AnthropicStreamEvent {
            r#type: "x".to_string(),
            content_block: None,
            index: None,
            delta: None,
            stop_reason: None,
        };
        assert!(p.process_event(&ev).is_empty());
    }

    // -- text_delta --------------------------------------------------

    /// `process_event` MUST emit `Content(Text)` chunks for
    /// `text_delta` events and accumulate the text.
    #[test]
    fn text_delta_emits_content_text_chunk() {
        let mut p = StreamProcessor::new();
        let ev = AnthropicStreamEvent {
            r#type: "content_block_delta".to_string(),
            content_block: None,
            index: Some(0),
            delta: Some(AnthropicStreamDelta {
                r#type: "text_delta".to_string(),
                text: Some("hello".to_string()),
                thinking: None,
                partial_json: None,
                signature: None,
            }),
            stop_reason: None,
        };
        let chunks = p.process_event(&ev);
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::Content(ContentPart::Text(t)) => {
                assert_eq!(t.text, "hello");
            }
            other => panic!("expected Content(Text), got {other:?}"),
        }
    }

    /// `process_event` MUST accumulate text across multiple
    /// `text_delta` events (used for streaming text output).
    #[test]
    fn text_delta_accumulates_across_events() {
        let mut p = StreamProcessor::new();
        for word in ["hello", " ", "world"] {
            let ev = AnthropicStreamEvent {
                r#type: "content_block_delta".to_string(),
                content_block: None,
                index: Some(0),
                delta: Some(AnthropicStreamDelta {
                    r#type: "text_delta".to_string(),
                    text: Some(word.to_string()),
                    thinking: None,
                    partial_json: None,
                    signature: None,
                }),
                stop_reason: None,
            };
            p.process_event(&ev);
        }
        // Finalize to inspect the aggregated text.
        let chunks = finalize(&mut p);
        match chunks.into_iter().next().unwrap() {
            StreamChunk::IsDone { result } => {
                assert_eq!(result.text, "hello world");
            }
            other => panic!("expected IsDone, got {other:?}"),
        }
    }

    // -- thinking_delta ----------------------------------------------

    /// `process_event` MUST emit `Content(Reasoning)` chunks for
    /// `thinking_delta` events and accumulate the reasoning.
    #[test]
    fn thinking_delta_emits_content_reasoning_chunk() {
        let mut p = StreamProcessor::new();
        let ev = AnthropicStreamEvent {
            r#type: "content_block_delta".to_string(),
            content_block: None,
            index: Some(0),
            delta: Some(AnthropicStreamDelta {
                r#type: "thinking_delta".to_string(),
                text: None,
                thinking: Some("step 1".to_string()),
                partial_json: None,
                signature: None,
            }),
            stop_reason: None,
        };
        let chunks = p.process_event(&ev);
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::Content(ContentPart::Reasoning(r)) => {
                assert_eq!(r.text, "step 1");
                assert_eq!(r.signature, None);
            }
            other => panic!("expected Content(Reasoning), got {other:?}"),
        }
    }

    /// `signature_delta` MUST fold into the next reasoning
    /// chunk's `signature` field (no separate stream chunk).
    #[test]
    fn signature_delta_sets_next_reasoning_signature() {
        let mut p = StreamProcessor::new();
        // 1) signature_delta (no stream chunk emitted).
        let ev1 = AnthropicStreamEvent {
            r#type: "content_block_delta".to_string(),
            content_block: None,
            index: Some(0),
            delta: Some(AnthropicStreamDelta {
                r#type: "signature_delta".to_string(),
                text: None,
                thinking: None,
                partial_json: None,
                signature: Some("sig-abc".to_string()),
            }),
            stop_reason: None,
        };
        let chunks1 = p.process_event(&ev1);
        assert!(
            chunks1.is_empty(),
            "signature_delta emits no chunk (got {:?})",
            chunks1
        );
        // 2) thinking_delta picks up the signature.
        let ev2 = AnthropicStreamEvent {
            r#type: "content_block_delta".to_string(),
            content_block: None,
            index: Some(0),
            delta: Some(AnthropicStreamDelta {
                r#type: "thinking_delta".to_string(),
                text: None,
                thinking: Some("r".to_string()),
                partial_json: None,
                signature: None,
            }),
            stop_reason: None,
        };
        let chunks2 = p.process_event(&ev2);
        match &chunks2[0] {
            StreamChunk::Content(ContentPart::Reasoning(r)) => {
                assert_eq!(r.signature, Some("sig-abc".to_string()));
            }
            _ => panic!("expected Content(Reasoning)"),
        }
    }

    // -- tool_use lifecycle ------------------------------------------

    /// `content_block_start` (tool_use) MUST emit `ToolCallStart`
    /// and buffer the input.
    #[test]
    fn tool_use_start_emits_tool_call_start() {
        let mut p = StreamProcessor::new();
        let ev = AnthropicStreamEvent {
            r#type: "content_block_start".to_string(),
            content_block: Some(AnthropicStreamContentBlock {
                r#type: "tool_use".to_string(),
                id: Some("t-1".to_string()),
                name: Some("bash".to_string()),
                input: Some("{\"cmd\":\"ls\"}".to_string()),
                thinking: None,
            }),
            index: Some(0),
            delta: None,
            stop_reason: None,
        };
        let chunks = p.process_event(&ev);
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::ToolCallStart {
                id,
                name,
                arguments,
            } => {
                assert_eq!(id, "t-1");
                assert_eq!(name, "bash");
                assert_eq!(
                    *arguments,
                    serde_json::json!("{\"cmd\":\"ls\"}".to_string())
                );
            }
            other => panic!("expected ToolCallStart, got {other:?}"),
        }
    }

    /// `input_json_delta` MUST emit `ToolCallDelta` and
    /// append to the buffer.
    #[test]
    fn input_json_delta_accumulates() {
        let mut p = StreamProcessor::new();
        // Start the tool_use first.
        p.process_event(&AnthropicStreamEvent {
            r#type: "content_block_start".to_string(),
            content_block: Some(AnthropicStreamContentBlock {
                r#type: "tool_use".to_string(),
                id: Some("t-1".to_string()),
                name: Some("bash".to_string()),
                input: Some("".to_string()),
                thinking: None,
            }),
            index: Some(0),
            delta: None,
            stop_reason: None,
        });
        // Now send two deltas.
        let ev = AnthropicStreamEvent {
            r#type: "content_block_delta".to_string(),
            content_block: None,
            index: Some(0),
            delta: Some(AnthropicStreamDelta {
                r#type: "input_json_delta".to_string(),
                text: None,
                thinking: None,
                partial_json: Some("{\"cmd\":\"l".to_string()),
                signature: None,
            }),
            stop_reason: None,
        };
        let chunks = p.process_event(&ev);
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::ToolCallDelta {
                id,
                arguments_delta,
            } => {
                assert_eq!(id, "t-1");
                assert_eq!(arguments_delta, "{\"cmd\":\"l");
            }
            _ => panic!("expected ToolCallDelta"),
        }
    }

    /// `content_block_stop` MUST emit `ToolCallEnd` and
    /// finalize the tool_use (populating
    /// `SamplingResult.tool_calls` on `message_stop`).
    #[test]
    fn content_block_stop_finalizes_tool_use() {
        let mut p = StreamProcessor::new();
        // Start.
        p.process_event(&AnthropicStreamEvent {
            r#type: "content_block_start".to_string(),
            content_block: Some(AnthropicStreamContentBlock {
                r#type: "tool_use".to_string(),
                id: Some("t-1".to_string()),
                name: Some("bash".to_string()),
                input: Some("{\"cmd\":\"ls\"}".to_string()),
                thinking: None,
            }),
            index: Some(0),
            delta: None,
            stop_reason: None,
        });
        // Stop.
        let chunks = p.process_event(&AnthropicStreamEvent {
            r#type: "content_block_stop".to_string(),
            content_block: None,
            index: Some(0),
            delta: None,
            stop_reason: None,
        });
        assert_eq!(chunks.len(), 1);
        assert!(
            matches!(&chunks[0], StreamChunk::ToolCallEnd { id } if id == "t-1")
        );
    }

    // -- message_stop / finalize -------------------------------------

    /// `message_stop` MUST emit `IsDone { result }` with the
    /// default stop_reason `"end_turn"` when none was set.
    #[test]
    fn message_stop_default_stop_reason_is_end_turn() {
        let mut p = StreamProcessor::new();
        let chunks = p.process_event(&AnthropicStreamEvent {
            r#type: "message_stop".to_string(),
            content_block: None,
            index: None,
            delta: None,
            stop_reason: None,
        });
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            StreamChunk::IsDone { result } => {
                assert_eq!(result.stop_reason, Some("end_turn".to_string()));
            }
            _ => panic!("expected IsDone"),
        }
    }

    /// `message_stop` MUST respect an explicit stop_reason from
    /// a prior `message_delta` event.
    #[test]
    fn message_delta_then_stop_propagates_reason() {
        let mut p = StreamProcessor::new();
        p.process_event(&AnthropicStreamEvent {
            r#type: "message_delta".to_string(),
            content_block: None,
            index: None,
            delta: None,
            stop_reason: Some("tool_use".to_string()),
        });
        let chunks = p.process_event(&AnthropicStreamEvent {
            r#type: "message_stop".to_string(),
            content_block: None,
            index: None,
            delta: None,
            stop_reason: None,
        });
        match &chunks[0] {
            StreamChunk::IsDone { result } => {
                assert_eq!(result.stop_reason, Some("tool_use".to_string()));
            }
            _ => panic!("expected IsDone"),
        }
    }

    /// `message_stop` MUST drain any residual ThinkExtractor
    /// text into the final SamplingResult.
    #[test]
    fn message_stop_drains_residual_text() {
        let mut p = StreamProcessor::new();
        // Send a text_delta that contains an unbalanced
        // `<think>` marker so the ThinkExtractor is still
        // holding text at message_stop.
        p.process_event(&AnthropicStreamEvent {
            r#type: "content_block_delta".to_string(),
            content_block: None,
            index: Some(0),
            delta: Some(AnthropicStreamDelta {
                r#type: "text_delta".to_string(),
                text: Some("unbalanced think<".to_string()),
                thinking: None,
                partial_json: None,
                signature: None,
            }),
            stop_reason: None,
        });
        let chunks = p.process_event(&AnthropicStreamEvent {
            r#type: "message_stop".to_string(),
            content_block: None,
            index: None,
            delta: None,
            stop_reason: None,
        });
        match &chunks[0] {
            StreamChunk::IsDone { result } => {
                // The drain flushed the residual text into the
                // final result.
                assert!(
                    result.text.contains("think"),
                    "expected text drain (got {:?})",
                    result.text
                );
            }
            _ => panic!("expected IsDone"),
        }
    }

    // -- unknown event types -----------------------------------------

    /// `process_event` MUST silently drop unknown event
    /// types (no chunks emitted, no panic).
    #[test]
    fn unknown_event_type_emits_no_chunks() {
        let mut p = StreamProcessor::new();
        for t in ["ping", "x", "message_start", "wat"] {
            let chunks = p.process_event(&AnthropicStreamEvent {
                r#type: t.to_string(),
                content_block: None,
                index: None,
                delta: None,
                stop_reason: None,
            });
            assert!(chunks.is_empty(), "event {t:?} produced {:?}", chunks);
        }
    }

    // -- helpers -----------------------------------------------------

    /// Helper: drain a processor with a `message_stop` and
    /// return the produced chunks.
    fn finalize(p: &mut StreamProcessor) -> Vec<StreamChunk> {
        p.process_event(&AnthropicStreamEvent {
            r#type: "message_stop".to_string(),
            content_block: None,
            index: None,
            delta: None,
            stop_reason: None,
        })
    }
}
