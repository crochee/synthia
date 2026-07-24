//! The new [`StreamProcessorV2`] — used by `complete_with_stream`.
//! Emits incremental
//! `ToolCallStart { id, name, arguments }`,
//! `ToolCallDelta { id, arguments_delta }`, `ToolCallEnd { id }`,
//! and a terminal `IsDone { result: SamplingResult }` on
//! `message_stop`.

use std::collections::HashMap;

use tracing::warn;

use super::events::{AnthropicStreamDelta, AnthropicStreamEvent};
use crate::types::{
    ContentPart,
    ReasoningContent,
    SamplingResult,
    StreamChunk,
    TextContent,
    TokenUsage,
    ToolUse,
};

/// Per-content-block tool-use buffer for V2. We keep the raw
/// partial JSON as a string (not a parsed `serde_json::Value`) so
/// the delta can be emitted verbatim and the parser runs once at
/// the end inside `IsDone`.
struct V2ToolUseBuffer {
    id: String,
    name: String,
    input: String,
}

/// Stream processor that emits the new chunk model required by
/// `complete_with_stream`:
/// - `Content(Text)` for `text_delta`
/// - `Content(Reasoning)` for `thinking_delta`
/// - `ToolCallStart { id, name, arguments }` on `content_block_start` (tool_use)
/// - `ToolCallDelta { id, arguments_delta }` on `input_json_delta`
/// - `ToolCallEnd { id }` on `content_block_stop`
/// - `Usage(...)` and `IsDone { result }` on `message_delta` / `message_stop`
pub struct StreamProcessorV2 {
    tool_buffers: HashMap<usize, V2ToolUseBuffer>,
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
}

impl StreamProcessorV2 {
    pub fn new() -> Self {
        Self {
            tool_buffers: HashMap::with_capacity(4),
            text: String::new(),
            reasoning: String::new(),
            last_reasoning_signature: None,
            tool_calls: Vec::new(),
            usage: None,
            stop_reason: None,
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
                            V2ToolUseBuffer {
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
                            self.text.push_str(text);
                            chunks.push(StreamChunk::Content(
                                ContentPart::Text(TextContent {
                                    text: text.clone(),
                                    cache_control: None,
                                }),
                            ));
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
                if let Some(delta) = &event.delta
                    && let Some(usage) = delta_usage(delta)
                {
                    self.usage = Some(usage.clone());
                    chunks.push(StreamChunk::Usage(usage));
                }
                if let Some(reason) = &event.stop_reason {
                    self.stop_reason = Some(reason.clone());
                }
            }
            "message_stop" => {
                if event.stop_reason.is_some() {
                    self.stop_reason = event.stop_reason.clone();
                }
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
                };
                tracing::debug!(
                    target: "synthia_provider::anthropic::v2",
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

impl Default for StreamProcessorV2 {
    fn default() -> Self {
        Self::new()
    }
}

/// Best-effort parse of a tool-use input string into a JSON value.
/// Anthropic guarantees valid JSON in `input_json_delta` sequences, so
/// when parsing fails we return the raw string as a JSON string value
/// rather than erroring — the caller can decide how to handle a malformed
/// payload.
pub(super) fn parse_tool_input(raw: &str) -> serde_json::Value {
    if raw.trim().is_empty() {
        return serde_json::Value::Object(serde_json::Map::new());
    }
    serde_json::from_str(raw)
        .unwrap_or_else(|_| serde_json::Value::String(raw.to_string()))
}

pub(super) fn delta_usage(delta: &AnthropicStreamDelta) -> Option<TokenUsage> {
    // Anthropic's `message_delta` event places usage fields alongside
    // the delta struct. We do not model them in `AnthropicStreamDelta`
    // (which is for streaming deltas, not the turn-level usage block),
    // so we cannot extract them here without expanding the type.
    // Returning `None` is the correct conservative behaviour for PR1-M2;
    // the PR2 work on provider-level usage tracking will wire this up.
    let _ = delta;
    None
}
