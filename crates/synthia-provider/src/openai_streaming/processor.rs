use std::collections::HashMap;

use super::types::OpenAIDeltaResponse;
use crate::types::{
    ContentPart,
    SamplingResult,
    StreamChunk,
    TextContent,
    TokenUsage,
    ToolUse,
};

struct V2ToolUseBuffer {
    id: String,
    name: String,
    /// Raw partial JSON, accumulated as a string. We keep the raw form
    /// (not a parsed `serde_json::Value`) so the delta can be emitted
    /// verbatim and the parser runs once at the end inside `IsDone`.
    input: String,
}

pub struct OpenAIStreamProcessorV2 {
    tool_buffers: HashMap<usize, V2ToolUseBuffer>,
    /// Per-turn accumulators for the final `SamplingResult`.
    text: String,
    reasoning: String,
    tool_calls: Vec<ToolUse>,
    usage: Option<TokenUsage>,
    /// `true` after `IsDone` has been emitted; further events are
    /// ignored.
    finished: bool,
}

impl OpenAIStreamProcessorV2 {
    pub fn new() -> Self {
        Self {
            tool_buffers: HashMap::with_capacity(4),
            text: String::new(),
            reasoning: String::new(),
            tool_calls: Vec::new(),
            usage: None,
            finished: false,
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
                    TextContent {
                        text: reasoning.clone(),
                        cache_control: None,
                    },
                )));
            }
            if let Some(ref content) = delta.content
                && !content.is_empty()
            {
                self.text.push_str(content);
                chunks.push(StreamChunk::Content(ContentPart::Text(
                    TextContent {
                        text: content.clone(),
                        cache_control: None,
                    },
                )));
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
                            vacant.insert(V2ToolUseBuffer {
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
            for (index, buffer) in
                std::mem::take(&mut self.tool_buffers).into_iter()
            {
                let _ = index;
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

    fn emit_is_done(&mut self, _finish_reason: Option<String>) -> StreamChunk {
        self.finished = true;
        let usage = self.usage.clone().unwrap_or_default();
        let reasoning = std::mem::take(&mut self.reasoning);
        let text = std::mem::take(&mut self.text);
        let tool_calls = std::mem::take(&mut self.tool_calls);
        let result = SamplingResult {
            text,
            tool_calls,
            reasoning,
            usage,
        };
        tracing::debug!(
            target: "synthia_provider::openai::v2",
            text_len = result.text.len(),
            tool_calls = result.tool_calls.len(),
            "openai stream complete"
        );
        StreamChunk::IsDone {
            result: Box::new(result),
        }
    }
}

impl Default for OpenAIStreamProcessorV2 {
    fn default() -> Self {
        Self::new()
    }
}

/// Best-effort parse of a tool-use argument string into a JSON value.
/// OpenAI guarantees valid JSON in `tool_calls[].function.arguments`
/// when complete; mid-stream partials may be unparseable, in which
/// case we return the raw string as a JSON string value.
pub(super) fn parse_tool_input(raw: &str) -> serde_json::Value {
    if raw.trim().is_empty() {
        return serde_json::Value::Object(serde_json::Map::new());
    }
    serde_json::from_str(raw)
        .unwrap_or_else(|_| serde_json::Value::String(raw.to_string()))
}
