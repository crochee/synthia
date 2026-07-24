//! The legacy [`StreamProcessor`] — used by the deprecated
//! `ModelProvider::stream()` method. Emits
//! `Content(ContentPart::ToolUse)` with the full accumulated
//! input on every delta.

use std::collections::HashMap;

use tracing::warn;

use super::events::AnthropicStreamEvent;
use crate::types::{ContentPart, StreamChunk, TextContent, ToolUse};

/// Per-content-block tool-use buffer (private to this module).
struct ToolUseBuffer {
    id: String,
    name: String,
    input: String,
}

/// Legacy SSE stream processor.
pub struct StreamProcessor {
    tool_buffers: HashMap<usize, ToolUseBuffer>,
}

impl StreamProcessor {
    #[inline]
    pub fn new() -> Self {
        Self {
            tool_buffers: HashMap::with_capacity(4),
        }
    }

    #[inline]
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
                        chunks.push(StreamChunk::Content(
                            ContentPart::ToolUse(ToolUse {
                                id: id.clone(),
                                name: name.clone(),
                                input: serde_json::Value::String(input_str),
                            }),
                        ));
                    }
                    "thinking" => {
                        if let Some(thinking) = &block.thinking {
                            chunks.push(StreamChunk::Content(
                                ContentPart::Reasoning(TextContent {
                                    text: thinking.clone(),
                                    cache_control: None,
                                }),
                            ));
                        }
                    }
                    "redacted_thinking" => {
                        chunks.push(StreamChunk::Content(
                            ContentPart::Reasoning(TextContent {
                                text: "[Redacted by safety filter]".to_string(),
                                cache_control: None,
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
                            chunks.push(StreamChunk::Content(
                                ContentPart::Reasoning(TextContent {
                                    text: thinking.clone(),
                                    cache_control: None,
                                }),
                            ));
                        }
                    }
                    "input_json_delta" => {
                        if let (Some(partial), Some(buffer)) = (
                            delta.partial_json.as_ref(),
                            self.tool_buffers.get_mut(&index),
                        ) {
                            buffer.input.push_str(partial);
                            chunks.push(StreamChunk::Content(
                                ContentPart::ToolUse(ToolUse {
                                    id: buffer.id.clone(),
                                    name: buffer.name.clone(),
                                    input: serde_json::Value::String(
                                        buffer.input.clone(),
                                    ),
                                }),
                            ));
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                if let Some(index) = event.index {
                    let _buffer = self.tool_buffers.remove(&index);
                }
            }
            "message_stop" => {
                let reason = event.stop_reason.clone().unwrap_or_default();
                chunks.push(StreamChunk::Stop(reason));
            }
            _ => {}
        }

        chunks
    }

    #[inline]
    pub fn reset(&mut self) {
        self.tool_buffers.clear();
    }
}

impl Default for StreamProcessor {
    fn default() -> Self {
        Self::new()
    }
}
