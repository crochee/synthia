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
            // Upstream inconsistency guard: when
            // `stop_reason == "tool_use"` but the
            // content has no `tool_use` blocks
            // (only text — e.g. a model that
            // mentioned a tool but did not emit it),
            // fall back to joining the text rather
            // than dropping the response. Without
            // this fallback the agent would see an
            // empty response and crash.
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
            } else if tool_uses.len() == 1 {
                // Every entry in `tool_uses` was
                // collected as `ContentPart::ToolUse`
                // (line 33), so unwrap is sound here.
                if let ContentPart::ToolUse(tu) = tool_uses.remove(0) {
                    Content::Single(ContentPart::ToolUse(tu))
                } else {
                    // Defensive: the inner type is
                    // invariant, but rustc cannot
                    // prove the destructuring
                    // without this branch.
                    Content::Single(ContentPart::Text(TextContent {
                        text: String::new(),
                        cache_control: None,
                    }))
                }
            } else {
                Content::Multi(tool_uses)
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
            // `cached` means "cache HIT" — a positive number
            // of tokens served from cache. Anthropic reports
            // `cache_read_input_tokens = Some(0)` for a cache
            // miss on a cache-eligible prompt; treating
            // `is_some()` as "cached" would mis-classify
            // these as hits. We require `Some(n)` with
            // `n > 0` to flip the flag.
            cached: resp.usage.cache_read_input_tokens.is_some_and(|n| n > 0),
            stop_reason: resp.stop_reason.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        super::types::{
            AnthropicContentBlock,
            AnthropicResponse,
            AnthropicUsage,
        },
        core::AnthropicProvider,
    };
    use crate::{
        ModelConfig,
        types::{Content, ContentPart, TextContent},
    };

    fn provider() -> AnthropicProvider {
        AnthropicProvider::new(ModelConfig {
            name: "claude-3-5-sonnet-20241022".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 8192,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        })
    }

    fn make_response(
        content: Vec<AnthropicContentBlock>,
        stop_reason: Option<&str>,
    ) -> AnthropicResponse {
        AnthropicResponse {
            id: "msg_01".to_string(),
            model: "claude-3-5-sonnet-20241022".to_string(),
            content,
            usage: AnthropicUsage {
                input_tokens: 10,
                output_tokens: 5,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
            },
            stop_reason: stop_reason.map(|s| s.to_string()),
        }
    }

    // -- stop_reason != "tool_use" ---------------------------------

    /// `parse_response` MUST join text blocks with `\n` when
    /// `stop_reason != "tool_use"`.
    #[test]
    fn parse_response_text_only_joins_with_newline() {
        let p = provider();
        let resp = make_response(
            vec![
                AnthropicContentBlock::Text {
                    text: "hello".to_string(),
                    cache_control: None,
                },
                AnthropicContentBlock::Text {
                    text: "world".to_string(),
                    cache_control: None,
                },
            ],
            Some("end_turn"),
        );
        let result = p.parse_response(&resp, "claude-3-5-sonnet-20241022");
        match result.content {
            Content::Single(ContentPart::Text(TextContent {
                text, ..
            })) => {
                assert_eq!(text, "hello\nworld");
            }
            other => panic!("expected Single(Text), got {other:?}"),
        }
    }

    /// `parse_response` MUST use the model arg (NOT `resp.model`).
    #[test]
    fn parse_response_uses_model_arg() {
        let p = provider();
        let resp = make_response(
            vec![AnthropicContentBlock::Text {
                text: "x".to_string(),
                cache_control: None,
            }],
            Some("end_turn"),
        );
        let result = p.parse_response(&resp, "override-model");
        assert_eq!(result.model, "override-model");
    }

    // -- stop_reason == "tool_use" ---------------------------------

    /// `parse_response` MUST wrap a single tool_use block in
    /// `Content::Single(ContentPart::ToolUse(...))`.
    #[test]
    fn parse_response_single_tool_use_wraps_in_single() {
        let p = provider();
        let resp = make_response(
            vec![AnthropicContentBlock::ToolUse {
                id: "toolu_1".to_string(),
                name: "bash".to_string(),
                input: serde_json::json!({"cmd": "ls"}),
                cache_control: None,
            }],
            Some("tool_use"),
        );
        let result = p.parse_response(&resp, "claude-3-5-sonnet-20241022");
        match result.content {
            Content::Single(ContentPart::ToolUse(tu)) => {
                assert_eq!(tu.id, "toolu_1");
                assert_eq!(tu.name, "bash");
                assert_eq!(tu.input, serde_json::json!({"cmd": "ls"}));
            }
            other => panic!("expected Single(ToolUse), got {other:?}"),
        }
    }

    /// `parse_response` MUST wrap multiple tool_use blocks in
    /// `Content::Multi(...)`.
    #[test]
    fn parse_response_multi_tool_use_wraps_in_multi() {
        let p = provider();
        let resp = make_response(
            vec![
                AnthropicContentBlock::ToolUse {
                    id: "t1".to_string(),
                    name: "bash".to_string(),
                    input: serde_json::json!({}),
                    cache_control: None,
                },
                AnthropicContentBlock::ToolUse {
                    id: "t2".to_string(),
                    name: "grep".to_string(),
                    input: serde_json::json!({}),
                    cache_control: None,
                },
            ],
            Some("tool_use"),
        );
        let result = p.parse_response(&resp, "claude-3-5-sonnet-20241022");
        match result.content {
            Content::Multi(parts) => {
                assert_eq!(parts.len(), 2);
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    /// Upstream inconsistency guard: when `stop_reason == "tool_use"`
    /// but no `tool_use` blocks exist (only text), MUST fall back
    /// to joining text rather than returning an empty response.
    #[test]
    fn parse_response_tool_use_with_text_only_falls_back_to_text() {
        let p = provider();
        let resp = make_response(
            vec![AnthropicContentBlock::Text {
                text: "fallback text".to_string(),
                cache_control: None,
            }],
            Some("tool_use"),
        );
        let result = p.parse_response(&resp, "claude-3-5-sonnet-20241022");
        match result.content {
            Content::Single(ContentPart::Text(TextContent {
                text, ..
            })) => {
                assert_eq!(text, "fallback text");
            }
            other => panic!("expected Single(Text), got {other:?}"),
        }
    }

    /// ToolUse blocks are filtered out of non-tool_use responses —
    /// only Text blocks are joined.
    #[test]
    fn parse_response_filters_tool_use_in_text_mode() {
        let p = provider();
        let resp = make_response(
            vec![
                AnthropicContentBlock::Text {
                    text: "answer".to_string(),
                    cache_control: None,
                },
                AnthropicContentBlock::ToolUse {
                    id: "t1".to_string(),
                    name: "bash".to_string(),
                    input: serde_json::json!({}),
                    cache_control: None,
                },
            ],
            Some("end_turn"),
        );
        let result = p.parse_response(&resp, "claude-3-5-sonnet-20241022");
        match result.content {
            Content::Single(ContentPart::Text(TextContent {
                text, ..
            })) => {
                assert_eq!(text, "answer");
            }
            other => panic!("expected Single(Text), got {other:?}"),
        }
    }

    // -- usage / cached -------------------------------------------

    /// `usage` MUST map `input_tokens` → `prompt_tokens`,
    /// `output_tokens` → `completion_tokens`, and total is sum.
    #[test]
    fn parse_response_usage_maps_correctly() {
        let p = provider();
        let resp = AnthropicResponse {
            id: "msg_01".to_string(),
            model: "m".to_string(),
            content: vec![],
            usage: AnthropicUsage {
                input_tokens: 100,
                output_tokens: 50,
                cache_read_input_tokens: Some(20),
                cache_creation_input_tokens: Some(5),
            },
            stop_reason: Some("end_turn".to_string()),
        };
        let result = p.parse_response(&resp, "m");
        assert_eq!(result.usage.prompt_tokens, 100);
        assert_eq!(result.usage.completion_tokens, 50);
        assert_eq!(result.usage.total_tokens, 150);
        assert_eq!(result.usage.cached_prompt_tokens, Some(20));
        assert_eq!(result.usage.cache_read_tokens, Some(20));
        assert_eq!(result.usage.cache_write_tokens, Some(5));
    }

    /// `cached` MUST be `true` ONLY when `cache_read_input_tokens`
    /// is `Some(n)` with `n > 0` (NOT just `is_some`).
    #[test]
    fn parse_response_cached_flag_requires_positive_count() {
        let p = provider();
        // cache_read = Some(0) — must be false (cache miss).
        let resp = AnthropicResponse {
            id: "msg_01".to_string(),
            model: "m".to_string(),
            content: vec![],
            usage: AnthropicUsage {
                input_tokens: 0,
                output_tokens: 0,
                cache_read_input_tokens: Some(0),
                cache_creation_input_tokens: None,
            },
            stop_reason: Some("end_turn".to_string()),
        };
        let result = p.parse_response(&resp, "m");
        assert!(!result.cached, "Some(0) MUST NOT be a cache hit");
    }

    /// `cached` MUST be `true` when `cache_read_input_tokens > 0`.
    #[test]
    fn parse_response_cached_flag_true_for_positive() {
        let p = provider();
        let resp = AnthropicResponse {
            id: "msg_01".to_string(),
            model: "m".to_string(),
            content: vec![],
            usage: AnthropicUsage {
                input_tokens: 0,
                output_tokens: 0,
                cache_read_input_tokens: Some(1),
                cache_creation_input_tokens: None,
            },
            stop_reason: Some("end_turn".to_string()),
        };
        let result = p.parse_response(&resp, "m");
        assert!(result.cached);
    }

    /// `cached` MUST be `false` when `cache_read_input_tokens = None`.
    #[test]
    fn parse_response_cached_flag_false_for_none() {
        let p = provider();
        let resp = AnthropicResponse {
            id: "msg_01".to_string(),
            model: "m".to_string(),
            content: vec![],
            usage: AnthropicUsage {
                input_tokens: 0,
                output_tokens: 0,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
            },
            stop_reason: Some("end_turn".to_string()),
        };
        let result = p.parse_response(&resp, "m");
        assert!(!result.cached);
    }

    /// `stop_reason` MUST be forwarded verbatim.
    #[test]
    fn parse_response_forwards_stop_reason() {
        let p = provider();
        let resp = make_response(
            vec![AnthropicContentBlock::Text {
                text: "x".to_string(),
                cache_control: None,
            }],
            Some("max_tokens"),
        );
        let result = p.parse_response(&resp, "m");
        assert_eq!(result.stop_reason, Some("max_tokens".to_string()));
    }

    /// `id` MUST be forwarded verbatim.
    #[test]
    fn parse_response_forwards_id() {
        let p = provider();
        let resp = make_response(
            vec![AnthropicContentBlock::Text {
                text: "x".to_string(),
                cache_control: None,
            }],
            Some("end_turn"),
        );
        let result = p.parse_response(&resp, "m");
        assert_eq!(result.id, "msg_01");
    }
}
