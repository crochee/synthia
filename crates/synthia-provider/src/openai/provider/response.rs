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
            stop_reason: resp
                .choices
                .first()
                .and_then(|c| c.finish_reason.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for `OpenAICompatibleProvider::parse_response`.
    //!
    //! The pre-existing `provider_test.rs` integration
    //! suite covers the happy paths (mock-server round-trip
    //! with text and tool_calls), but the
    //! **input-argument parsing** edge cases (the
    //! `serde_json::from_str` fallback branches at lines
    //! 47-53 and 86-94) have no direct unit coverage. This
    //! module pins down:
    //!
    //! - `tool_calls[].function.arguments` parses
    //!   correctly when the wire string is valid JSON
    //!   (object).
    //! - When the wire string is **not** valid JSON (e.g.
    //!   upstream sent `arguments = "{not valid"`), the
    //!   parser falls back to wrapping the raw string in
    //!   `Value::String(s)` — NOT silently dropping the
    //!   call or panicking.
    //! - When the response is empty (`content = None`,
    //!   `tool_calls = None`), an empty-text placeholder
    //!   is returned.
    use super::{
        super::super::types::{
            OpenAIChoice,
            OpenAIContentPart,
            OpenAIResponse,
            OpenAIToolUse,
            OpenAIToolUseFunction,
            OpenAIUsage,
        },
        OpenAICompatibleProvider,
    };
    use crate::ModelConfig;

    fn provider() -> OpenAICompatibleProvider {
        OpenAICompatibleProvider::new(
            "http://localhost:0".to_string(),
            ModelConfig {
                name: "test-model".to_string(),
                provider: "openai".to_string(),
                context_window: 4096,
                max_output_tokens: 1024,
                supports_tools: true,
                supports_streaming: true,
                supports_reasoning: false,
            },
        )
    }

    fn empty_usage() -> OpenAIUsage {
        OpenAIUsage {
            prompt_tokens: 1,
            completion_tokens: 1,
            total_tokens: 2,
        }
    }

    fn response_with_tool_calls(arguments: &str) -> OpenAIResponse {
        OpenAIResponse {
            id: "r1".to_string(),
            model: "test-model".to_string(),
            choices: vec![OpenAIChoice {
                message: super::super::super::types::OpenAIMessage {
                    role: "assistant".to_string(),
                    content: None,
                    tool_calls: Some(vec![OpenAIToolUse {
                        id: "call_1".to_string(),
                        r#type: "function".to_string(),
                        function: OpenAIToolUseFunction {
                            name: "get_weather".to_string(),
                            arguments: arguments.to_string(),
                        },
                    }]),
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                    reasoning: None,
                },
                finish_reason: Some("tool_calls".to_string()),
            }],
            usage: empty_usage(),
        }
    }

    /// Happy path: `arguments` is a JSON object string,
    /// parsed into the matching `serde_json::Value`.
    #[test]
    fn parse_response_tool_call_arguments_object_parses_to_object() {
        let p = provider();
        let resp = response_with_tool_calls(r#"{"location":"Beijing"}"#);
        let parsed = p.parse_response(&resp);
        // `Content::parts([one])` collapses to `Single`,
        // so the single-tool-call case is `Single(ToolUse)`.
        match parsed.content {
            crate::types::Content::Single(
                crate::types::ContentPart::ToolUse(tc),
            ) => {
                assert_eq!(tc.id, "call_1");
                assert_eq!(tc.name, "get_weather");
                assert_eq!(
                    tc.input,
                    serde_json::json!({"location": "Beijing"})
                );
            }
            other => panic!("expected Single(ToolUse), got {other:?}"),
        }
    }

    /// Regression: when `arguments` is NOT valid JSON
    /// (e.g. malformed upstream), the parser MUST NOT
    /// panic, MUST NOT drop the call, and MUST surface
    /// the raw string wrapped in `Value::String`. This
    /// lets the downstream tool's argument validator
    /// emit a sensible "invalid JSON" error instead of
    /// the agent loop silently seeing an empty
    /// `tool_calls`.
    #[test]
    fn parse_response_tool_call_arguments_invalid_json_falls_back_to_string() {
        let p = provider();
        // Malformed — missing closing quote and brace.
        let resp = response_with_tool_calls(r#"{not valid"#);
        let parsed = p.parse_response(&resp);
        match parsed.content {
            crate::types::Content::Single(
                crate::types::ContentPart::ToolUse(tc),
            ) => {
                assert_eq!(tc.id, "call_1");
                assert_eq!(tc.name, "get_weather");
                // The raw string is preserved verbatim,
                // wrapped in Value::String.
                assert_eq!(
                    tc.input,
                    serde_json::Value::String("{not valid".to_string())
                );
            }
            other => panic!("expected Single(ToolUse), got {other:?}"),
        }
    }

    /// Empty response (no content, no tool_calls) yields
    /// an empty-text placeholder — never `Multi([])` and
    /// never panic.
    #[test]
    fn parse_response_empty_response_yields_empty_text_placeholder() {
        let p = provider();
        let resp = OpenAIResponse {
            id: "r1".to_string(),
            model: "test-model".to_string(),
            choices: vec![OpenAIChoice {
                message: super::super::super::types::OpenAIMessage {
                    role: "assistant".to_string(),
                    content: None,
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                    reasoning: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: empty_usage(),
        };
        let parsed = p.parse_response(&resp);
        match &parsed.content {
            crate::types::Content::Single(crate::types::ContentPart::Text(
                t,
            )) => assert!(t.text.is_empty()),
            other => panic!("expected Single(Text(empty)), got {other:?}"),
        }
    }

    /// Tool call with `arguments = "{}"` (empty object)
    /// is the documented "no-argument tool call" idiom
    /// and MUST parse to an empty `Value::Object`, not a
    /// string fallback. This guards against a refactor
    /// that confuses empty string with empty object.
    #[test]
    fn parse_response_tool_call_arguments_empty_object_parses_to_empty_object()
    {
        let p = provider();
        let resp = response_with_tool_calls("{}");
        let parsed = p.parse_response(&resp);
        match parsed.content {
            crate::types::Content::Single(
                crate::types::ContentPart::ToolUse(tc),
            ) => {
                assert_eq!(tc.input, serde_json::json!({}));
            }
            other => panic!("expected Single(ToolUse), got {other:?}"),
        }
    }

    /// Empty `choices: vec![]` — `choices.first()`
    /// returns `None`, so every branch filter fails
    /// and the function falls into the `else` arm
    /// that yields `Content::Single(Text(""))`.
    /// This is the documented defensive behavior for
    /// upstream-bug responses (some gateways send an
    /// empty array when the upstream request was
    /// rejected AFTER the request body was logged).
    /// Pin this down: a refactor that uses `unwrap()`
    /// on `choices.first()` would panic on every
    /// such response.
    #[test]
    fn parse_response_with_empty_choices_returns_empty_text_placeholder() {
        let p = provider();
        let resp = OpenAIResponse {
            id: "r-empty".to_string(),
            model: "test-model".to_string(),
            choices: vec![],
            usage: empty_usage(),
        };
        let parsed = p.parse_response(&resp);
        match &parsed.content {
            crate::types::Content::Single(crate::types::ContentPart::Text(
                t,
            )) => assert!(
                t.text.is_empty(),
                "empty choices MUST yield empty text; got {:?}",
                t.text
            ),
            other => panic!("expected Single(Text(\"\")), got {other:?}"),
        }
        // `stop_reason` MUST be `None` — there is no
        // choice to draw it from.
        assert!(
            parsed.stop_reason.is_none(),
            "empty choices must yield stop_reason=None; got {:?}",
            parsed.stop_reason
        );
        // Usage propagates verbatim — the response was
        // empty in content but the upstream DID log
        // token usage for the failed request.
        assert_eq!(parsed.usage.total_tokens, 2);
    }

    /// When a single choice has `finish_reason:
    /// None` (e.g. a synthetic mock response missing
    /// the field), the `stop_reason` field on
    /// `CompletionResponse` MUST be `None`, not an
    /// empty string and not a panic. Pin the contract
    /// for the `and_then(|c| c.finish_reason.clone())`
    /// path at line 130.
    #[test]
    fn parse_response_with_finish_reason_none_yields_stop_reason_none() {
        let p = provider();
        let resp = OpenAIResponse {
            id: "r-fr-none".to_string(),
            model: "test-model".to_string(),
            choices: vec![OpenAIChoice {
                message: super::super::super::types::OpenAIMessage {
                    role: "assistant".to_string(),
                    content: Some(vec![OpenAIContentPart::Text {
                        text: "ok".to_string(),
                    }]),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                    reasoning: None,
                },
                finish_reason: None,
            }],
            usage: empty_usage(),
        };
        let parsed = p.parse_response(&resp);
        assert!(
            parsed.stop_reason.is_none(),
            "finish_reason=None must propagate as stop_reason=None; got {:?}",
            parsed.stop_reason
        );
        // Text content still surfaces correctly.
        match &parsed.content {
            crate::types::Content::Single(crate::types::ContentPart::Text(
                t,
            )) => assert_eq!(t.text, "ok"),
            other => panic!("expected Single(Text(\"ok\")), got {other:?}"),
        }
    }
}
