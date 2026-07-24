//! The response-parsing method on
//! [`super::core::AnthropicProvider`]:
//!
//! - [`AnthropicProvider::parse_response`] — takes an
//!   [`super::super::types::AnthropicResponse`] and produces
//!   a [`crate::types::CompletionResponse`]. When
//!   `stop_reason` is `"tool_use"`, tool-use blocks are
//!   extracted; otherwise the text content is joined.

use super::{super::types::AnthropicContentBlock, core::AnthropicProvider};
use crate::types::{
    CompletionResponse,
    Content,
    ContentPart,
    TextContent,
    TokenUsage,
    ToolUse,
};

impl AnthropicProvider {
    pub(in crate::anthropic) fn parse_response(
        &self,
        resp: &super::super::types::AnthropicResponse,
        model: &str,
    ) -> CompletionResponse {
        let content = if resp.stop_reason.as_deref() == Some("tool_use") {
            let mut tool_uses: Vec<_> = resp
                .content
                .iter()
                .filter_map(|c| match c {
                    AnthropicContentBlock::ToolUse {
                        id, name, input, ..
                    } => Some(ContentPart::ToolUse(ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    })),
                    _ => None,
                })
                .collect();
            if tool_uses.is_empty() {
                let text = resp
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        AnthropicContentBlock::Text { text, .. } => {
                            Some(text.clone())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Content::Single(ContentPart::Text(TextContent {
                    text,
                    cache_control: None,
                }))
            } else {
                let calls_count = tool_uses.len();
                match calls_count {
                    0 => Content::Single(ContentPart::Text(TextContent {
                        text: String::new(),
                        cache_control: None,
                    })),
                    1 => {
                        let tc = tool_uses.remove(0);
                        if let ContentPart::ToolUse(tu) = tc {
                            Content::Single(ContentPart::ToolUse(tu))
                        } else {
                            Content::Single(ContentPart::Text(TextContent {
                                text: String::new(),
                                cache_control: None,
                            }))
                        }
                    }
                    _ => Content::Multi(tool_uses),
                }
            }
        } else {
            let text = resp
                .content
                .iter()
                .filter_map(|c| match c {
                    AnthropicContentBlock::Text { text, .. } => {
                        Some(text.clone())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            Content::Single(ContentPart::Text(TextContent {
                text,
                cache_control: None,
            }))
        };

        CompletionResponse {
            id: resp.id.clone(),
            model: model.to_string(),
            content,
            usage: TokenUsage {
                prompt_tokens: resp.usage.input_tokens,
                completion_tokens: resp.usage.output_tokens,
                total_tokens: resp.usage.input_tokens
                    + resp.usage.output_tokens,
                cached_prompt_tokens: resp.usage.cache_read_input_tokens,
                cache_read_tokens: resp.usage.cache_read_input_tokens,
                cache_write_tokens: resp.usage.cache_creation_input_tokens,
            },
            cached: resp.usage.cache_read_input_tokens.is_some(),
        }
    }
}
