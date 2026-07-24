//! The response parser on
//! [`super::core::OpenAICompatibleProvider`]:
//!
//! - [`OpenAICompatibleProvider::parse_response`] — takes
//!   an [`super::types::OpenAIResponse`] and produces a
//!   [`crate::types::CompletionResponse`]. The
//!   `choices[0].message.content` (if non-empty) takes
//!   precedence; otherwise the
//!   `choices[0].message.tool_calls` (if any) is converted
//!   into `Content::parts(vec![ContentPart::ToolUse(..)])`;
//!   otherwise the response is an empty single-text
//!   placeholder.

use super::{super::types::OpenAIContentPart, core::OpenAICompatibleProvider};
use crate::types::{
    CompletionResponse,
    Content,
    ContentPart,
    TextContent,
    TokenUsage,
    ToolUse,
};

impl OpenAICompatibleProvider {
    pub(in crate::openai) fn parse_response(
        &self,
        resp: &super::super::types::OpenAIResponse,
    ) -> CompletionResponse {
        let choice = resp.choices.first();

        let content = if let Some(c) = choice.filter(|c| {
            c.message.content.as_ref().is_some_and(|v| !v.is_empty())
        }) {
            if let Some(content_vec) = c.message.content.as_deref() {
                let parts: Vec<ContentPart> = content_vec
                    .iter()
                    .filter_map(|part| match part {
                        OpenAIContentPart::Text { text }
                            if !text.is_empty() =>
                        {
                            Some(ContentPart::Text(TextContent {
                                text: text.clone(),
                                cache_control: None,
                            }))
                        }
                        OpenAIContentPart::ToolUse { id, name, input } => {
                            let parsed_input =
                                if let serde_json::Value::String(s) = input {
                                    serde_json::from_str(s)
                                        .unwrap_or_else(|_| input.clone())
                                } else {
                                    input.clone()
                                };
                            Some(ContentPart::ToolUse(ToolUse {
                                id: id.clone(),
                                name: name.clone(),
                                input: parsed_input,
                            }))
                        }
                        _ => None,
                    })
                    .collect();

                if parts.is_empty() {
                    Content::Single(ContentPart::Text(TextContent {
                        text: String::new(),
                        cache_control: None,
                    }))
                } else {
                    Content::parts(parts)
                }
            } else {
                Content::Single(ContentPart::Text(TextContent {
                    text: String::new(),
                    cache_control: None,
                }))
            }
        } else if let Some(c) = choice.filter(|c| {
            c.message.tool_calls.as_ref().is_some_and(|v| !v.is_empty())
        }) {
            if let Some(tool_calls) = c.message.tool_calls.as_deref() {
                Content::parts(
                    tool_calls
                        .iter()
                        .map(|tc| {
                            let parsed_input =
                                serde_json::from_str::<serde_json::Value>(
                                    &tc.function.arguments,
                                )
                                .unwrap_or_else(|_| {
                                    serde_json::Value::String(
                                        tc.function.arguments.clone(),
                                    )
                                });
                            ToolUse {
                                id: tc.id.clone(),
                                name: tc.function.name.clone(),
                                input: parsed_input,
                            }
                        })
                        .map(ContentPart::ToolUse)
                        .collect(),
                )
            } else {
                Content::Single(ContentPart::Text(TextContent {
                    text: String::new(),
                    cache_control: None,
                }))
            }
        } else {
            Content::Single(ContentPart::Text(TextContent {
                text: String::new(),
                cache_control: None,
            }))
        };

        CompletionResponse {
            id: resp.id.clone(),
            model: resp.model.clone(),
            content,
            usage: TokenUsage {
                prompt_tokens: resp.usage.prompt_tokens,
                completion_tokens: resp.usage.completion_tokens,
                total_tokens: resp.usage.total_tokens,
                cached_prompt_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
            },
            cached: false,
        }
    }
}
