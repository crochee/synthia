//! `AnthropicProvider` — the stateful struct + its
//! request/response transformation helpers. The
//! `ModelProvider` trait impl lives in `traits_impl`.
//!
//! # Module Layout
//!
//! - `core`: [`core::AnthropicProvider`] struct +
//!   `new` / `with_api_key` / `with_base_url` constructors.
//! - `transform`: 5 transform methods
//!   ([`transform::AnthropicProvider::transform_request`],
//!   `transform_message`, `transform_part`,
//!   `reorder_anthropic_messages`, `sanitize_tool_id`).
//! - `parse`:
//!   [`parse::AnthropicProvider::parse_response`].
//! - `request`:
//!   [`request::AnthropicProvider::make_request`].

mod core;
mod parse;
mod request;
mod transform;

pub use core::AnthropicProvider;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        anthropic::{
            AnthropicProvider,
            types::{AnthropicContentBlock, AnthropicResponse, AnthropicUsage},
        },
        cache_mark::{CacheControlMark, CacheScope, CacheTtl},
        cache_policy::CachePolicy,
        types::{
            CompletionRequest,
            Content,
            ContentPart,
            Message,
            ModelConfig,
            Role,
            TextContent,
            ToolChoice,
            ToolDefinition,
        },
    };

    fn make_provider() -> AnthropicProvider {
        AnthropicProvider::new(ModelConfig {
            name: "claude-3".into(),
            provider: "anthropic".into(),
            context_window: 200_000,
            max_output_tokens: 8_192,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        })
    }

    #[test]
    fn transform_request_with_none_cache_policy_preserves_text_system() {
        let provider = make_provider();
        let req = CompletionRequest {
            model: "claude-3".to_string(),
            messages: Arc::new(vec![
                Message::system("You are helpful."),
                Message::user("hi"),
            ]),
            tools: Arc::new(vec![]),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
        };
        let anthropic_req = provider.transform_request(&req);
        // system should be Text variant (serializes as plain string).
        let sys_json = serde_json::to_value(&anthropic_req.system).unwrap();
        assert_eq!(sys_json, serde_json::json!("You are helpful."));
    }

    #[test]
    fn transform_request_with_some_cache_policy_injects_on_last_tool() {
        let provider = make_provider();
        let req = CompletionRequest {
            model: "claude-3".to_string(),
            messages: Arc::new(vec![]),
            tools: Arc::new(vec![
                ToolDefinition::new("tool_a", "A", serde_json::json!({})),
                ToolDefinition::new("tool_b", "B", serde_json::json!({})),
            ]),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: Some(CachePolicy::default()),
        };
        let anthropic_req = provider.transform_request(&req);
        let tools = anthropic_req.tools.unwrap();
        assert!(tools[0].cache_control.is_none());
        assert!(tools[1].cache_control.is_some());
        let cc_json = serde_json::to_value(&tools[1].cache_control).unwrap();
        assert_eq!(cc_json, serde_json::json!({"type": "ephemeral"}));
    }

    #[test]
    fn transform_request_with_some_cache_policy_injects_structured_system() {
        let provider = make_provider();
        let req = CompletionRequest {
            model: "claude-3".to_string(),
            messages: Arc::new(vec![Message::system("You are helpful.")]),
            tools: Arc::new(vec![]),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: Some(CachePolicy::default()),
        };
        let anthropic_req = provider.transform_request(&req);
        // system should be Structured variant with cache_control.
        let sys_json = serde_json::to_value(&anthropic_req.system).unwrap();
        assert!(sys_json.is_array());
        assert_eq!(sys_json[0]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn transform_request_with_some_cache_policy_injects_on_last_user_message() {
        let provider = make_provider();
        let req = CompletionRequest {
            model: "claude-3".to_string(),
            messages: Arc::new(vec![
                Message::user("first"),
                Message::assistant("response"),
                Message::user("second"),
            ]),
            tools: Arc::new(vec![]),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: Some(CachePolicy::default()),
        };
        let anthropic_req = provider.transform_request(&req);
        // Last user message ("second") is the last message after filtering
        // system + reordering; its Text block must carry cache_control.
        let last_msg = anthropic_req.messages.last().unwrap();
        let last_block = last_msg.content.last().unwrap();
        match last_block {
            AnthropicContentBlock::Text { cache_control, .. } => {
                assert!(cache_control.is_some());
            }
            other => panic!("expected Text block, got {other:?}"),
        }
        // The earlier user message ("first") must NOT be marked.
        let first_msg = &anthropic_req.messages[0];
        match &first_msg.content[0] {
            AnthropicContentBlock::Text { cache_control, .. } => {
                assert!(cache_control.is_none());
            }
            other => panic!("expected Text block, got {other:?}"),
        }
    }

    /// Build a `CompletionRequest` skeleton with no cache policy and a
    /// single tool. `cache_policy: None` ensures `apply_cache_policy` is
    /// not invoked, so marks set directly on tools / messages survive into
    /// `transform_request` (which is what we want to test).
    fn make_request_no_policy() -> CompletionRequest {
        CompletionRequest {
            model: "claude-3".to_string(),
            messages: Arc::new(vec![]),
            tools: Arc::new(vec![ToolDefinition::new(
                "tool_a",
                "A",
                serde_json::json!({"type": "object"}),
            )]),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
        }
    }

    #[test]
    fn transform_request_propagates_non_default_scope_to_tool_cache_control() {
        let provider = make_provider();
        let mut req = make_request_no_policy();
        Arc::make_mut(&mut req.tools)[0].cache_control =
            Some(CacheControlMark {
                ttl: CacheTtl::Ephemeral,
                scope: CacheScope::new("alice", "s1"),
                pinned: false,
            });
        let anthropic_req = provider.transform_request(&req);
        let tools = anthropic_req.tools.unwrap();
        let cc_json = serde_json::to_value(&tools[0].cache_control).unwrap();
        assert_eq!(cc_json["type"], "ephemeral");
        assert_eq!(cc_json["cache_namespace"], "u=alice;s=s1");
    }

    #[test]
    fn transform_request_default_scope_omits_namespace_field() {
        let provider = make_provider();
        let mut req = make_request_no_policy();
        Arc::make_mut(&mut req.tools)[0].cache_control =
            Some(CacheControlMark::default());
        let anthropic_req = provider.transform_request(&req);
        let tools = anthropic_req.tools.unwrap();
        let cc_json = serde_json::to_value(&tools[0].cache_control).unwrap();
        // Default (anonymous) scope produces byte-identical pre-change JSON.
        assert_eq!(cc_json, serde_json::json!({"type": "ephemeral"}));
    }

    #[test]
    fn transform_request_different_scopes_produce_different_cache_control() {
        let provider = make_provider();
        let mk_req = |scope: CacheScope| {
            let mut req = make_request_no_policy();
            Arc::make_mut(&mut req.tools)[0].cache_control =
                Some(CacheControlMark {
                    ttl: CacheTtl::Long,
                    scope,
                    pinned: true,
                });
            req
        };
        let cc_a = serde_json::to_value(
            &provider
                .transform_request(&mk_req(CacheScope::new("alice", "s1")))
                .tools
                .unwrap()[0]
                .cache_control,
        )
        .unwrap();
        let cc_b = serde_json::to_value(
            &provider
                .transform_request(&mk_req(CacheScope::new("bob", "s1")))
                .tools
                .unwrap()[0]
                .cache_control,
        )
        .unwrap();
        assert_ne!(cc_a, cc_b);
        assert_eq!(cc_a["cache_namespace"], "u=alice;s=s1");
        assert_eq!(cc_b["cache_namespace"], "u=bob;s=s1");
        // TTL is preserved alongside the namespace.
        assert_eq!(cc_a["ttl_seconds"], 3600);
        assert_eq!(cc_b["ttl_seconds"], 3600);
    }

    #[test]
    fn transform_request_propagates_scope_to_user_message_cache_control() {
        let provider = make_provider();
        let mut req = make_request_no_policy();
        Arc::make_mut(&mut req.tools).clear();
        req.messages = Arc::new(vec![Message::new(
            Role::User,
            Content::Single(ContentPart::Text(TextContent {
                text: "hi".to_string(),
                cache_control: Some(CacheControlMark {
                    ttl: CacheTtl::Ephemeral,
                    scope: CacheScope::new("alice", "s1"),
                    pinned: false,
                }),
            })),
        )]);
        let anthropic_req = provider.transform_request(&req);
        let last_msg = anthropic_req.messages.last().unwrap();
        match &last_msg.content[0] {
            AnthropicContentBlock::Text { cache_control, .. } => {
                let cc_json = serde_json::to_value(cache_control).unwrap();
                assert_eq!(cc_json["type"], "ephemeral");
                assert_eq!(cc_json["cache_namespace"], "u=alice;s=s1");
            }
            other => panic!("expected Text block, got {other:?}"),
        }
    }

    #[test]
    fn transform_request_propagates_scope_to_system_cache_control() {
        let provider = make_provider();
        // `tools: false` so apply_cache_policy does NOT overwrite the
        // directly-set mark on tool_a; `system: true` selects the Structured
        // system variant so a cache_control is attached to the system block.
        let policy = CachePolicy {
            tools: false,
            ..CachePolicy::default()
        };
        let mut req = CompletionRequest {
            model: "claude-3".to_string(),
            messages: Arc::new(vec![Message::system("You are helpful.")]),
            tools: Arc::new(vec![ToolDefinition::new(
                "tool_a",
                "A",
                serde_json::json!({"type": "object"}),
            )]),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: Some(policy),
        };
        Arc::make_mut(&mut req.tools)[0].cache_control =
            Some(CacheControlMark {
                ttl: CacheTtl::Ephemeral,
                scope: CacheScope::new("alice", "s1"),
                pinned: false,
            });
        let anthropic_req = provider.transform_request(&req);
        let sys_json = serde_json::to_value(&anthropic_req.system).unwrap();
        assert!(sys_json.is_array());
        assert_eq!(sys_json[0]["cache_control"]["type"], "ephemeral");
        // The system block inherits the scope from the representative mark
        // on tool_a, so two users with identical system prompts get
        // namespaced cache_control JSON.
        assert_eq!(
            sys_json[0]["cache_control"]["cache_namespace"],
            "u=alice;s=s1"
        );
    }

    /// Edge case: a request with NO `Role::System` message
    /// but with `cache_policy.system = true`. The provider
    /// must:
    /// 1. NOT synthesise an empty system block just because
    ///    caching is requested — that would inject
    ///    `{"type":"text","text":""}` and leak the
    ///    default cache namespace.
    /// 2. Leave `anthropic_req.system = None` (the
    ///    consumer-side absence).
    /// 3. Still mark the last tool / last user message as
    ///    cacheable (this part is already covered by the
    ///    sibling tests; we re-verify the tool path is
    ///    unaffected by the missing system).
    #[test]
    fn transform_request_without_system_message_yields_no_system_block_even_when_policy_requested()
     {
        let provider = make_provider();
        let req = CompletionRequest {
            model: "claude-3".to_string(),
            // NO system message, only user + assistant.
            messages: Arc::new(vec![
                Message::user("hi"),
                Message::assistant("hello"),
            ]),
            tools: Arc::new(vec![ToolDefinition::new(
                "demo",
                "Demo tool",
                serde_json::json!({"type":"object"}),
            )]),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            // Cache policy IS enabled — system caching
            // requested, but there is no system to cache.
            cache_policy: Some(CachePolicy::default()),
        };
        let anthropic_req = provider.transform_request(&req);

        // System block must be absent — `system_text?`
        // returns `None` in `build_anthropic_system`, so
        // `system = None` propagates up.
        assert!(
            anthropic_req.system.is_none(),
            "system block must be None when no Role::System message is present; got {:?}",
            anthropic_req.system
        );

        // Tool block must still be present and marked for
        // cache (policy.tools default is true).
        let tools = anthropic_req
            .tools
            .as_ref()
            .expect("tools must be present when not empty");
        assert_eq!(tools.len(), 1);
        assert_eq!(
            serde_json::to_value(&tools[0].cache_control)
                .unwrap()
                .get("type"),
            Some(&serde_json::json!("ephemeral")),
            "tool cache_control must still be applied even when system is absent"
        );
    }

    #[test]
    fn parse_response_populates_cache_read_and_write_tokens() {
        let provider = make_provider();
        let resp = AnthropicResponse {
            id: "msg-1".to_string(),
            model: "claude-3".to_string(),
            content: vec![AnthropicContentBlock::Text {
                text: "hello".to_string(),
                cache_control: None,
            }],
            usage: AnthropicUsage {
                input_tokens: 100,
                output_tokens: 50,
                cache_read_input_tokens: Some(80),
                cache_creation_input_tokens: Some(20),
            },
            stop_reason: None,
        };
        let parsed = provider.parse_response(&resp, "claude-3");
        assert_eq!(parsed.usage.prompt_tokens, 100);
        assert_eq!(parsed.usage.completion_tokens, 50);
        assert_eq!(parsed.usage.cache_read_tokens, Some(80));
        assert_eq!(parsed.usage.cache_write_tokens, Some(20));
        // `cached_prompt_tokens` mirrors `cache_read_tokens` for back-compat.
        assert_eq!(parsed.usage.cached_prompt_tokens, Some(80));
        assert!(parsed.cached);
    }

    #[test]
    fn parse_response_cache_tokens_none_when_unreported() {
        let provider = make_provider();
        let resp = AnthropicResponse {
            id: "msg-2".to_string(),
            model: "claude-3".to_string(),
            content: vec![AnthropicContentBlock::Text {
                text: "no cache here".to_string(),
                cache_control: None,
            }],
            usage: crate::anthropic::types::AnthropicUsage {
                input_tokens: 100,
                output_tokens: 50,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
            },
            stop_reason: None,
        };
        let parsed = provider.parse_response(&resp, "claude-3");
        assert_eq!(parsed.usage.cache_read_tokens, None);
        assert_eq!(parsed.usage.cache_write_tokens, None);
        assert!(!parsed.cached);
    }

    /// Regression test for a subtle `cached` flag bug.
    ///
    /// Anthropic reports `cache_read_input_tokens = Some(0)`
    /// when the request was cache-eligible but landed on a
    /// cache MISS (no tokens were served from cache). The
    /// previous implementation used
    /// `cache_read_input_tokens.is_some()` which would
    /// mis-classify `Some(0)` as a cache HIT — inflating
    /// cache-hit rates on observability dashboards and
    /// misleading downstream `cached`-consumers.
    ///
    /// The fix requires `Some(n)` with `n > 0` to flip the
    /// flag. `Some(0)` must produce `cached = false`,
    /// matching the semantic of "no tokens were served from
    /// cache".
    #[test]
    fn parse_response_cache_miss_with_some_zero_does_not_mark_cached() {
        let provider = make_provider();
        let resp = AnthropicResponse {
            id: "msg-zero-miss".to_string(),
            model: "claude-3".to_string(),
            content: vec![AnthropicContentBlock::Text {
                text: "miss".to_string(),
                cache_control: None,
            }],
            usage: crate::anthropic::types::AnthropicUsage {
                input_tokens: 100,
                output_tokens: 50,
                // Cache-eligible but missed: Anthropic
                // reports Some(0).
                cache_read_input_tokens: Some(0),
                cache_creation_input_tokens: Some(20),
            },
            stop_reason: None,
        };
        let parsed = provider.parse_response(&resp, "claude-3");
        // Token counts propagate verbatim (Some(0) is a
        // legal Anthropic value).
        assert_eq!(parsed.usage.cache_read_tokens, Some(0));
        assert_eq!(parsed.usage.cache_write_tokens, Some(20));
        // But `cached` must be FALSE — zero tokens
        // served from cache is a miss, not a hit.
        assert!(
            !parsed.cached,
            "Some(0) cache_read_input_tokens must NOT mark `cached = true` \
             (this was the bug): cached={}",
            parsed.cached
        );
    }

    /// `parse_response` has a SECONDARY contract on
    /// `stop_reason == Some("tool_use")`. Content is
    /// reduced to the ToolUse blocks only (any text
    /// block is dropped, not joined). The wire
    /// `stop_reason` is propagated to the
    /// `CompletionResponse.stop_reason` field (see
    /// dead-state fix in the OpenAI/Anthropic
    /// propagation pass). Without this test a refactor
    /// that joins text content under "tool_use"
    /// would pass the cache tests above but silently
    /// lose the tool calls on the agent side.
    #[test]
    fn parse_response_with_tool_use_stop_reason_extracts_only_tool_blocks() {
        use crate::types::ToolUse;
        let provider = make_provider();
        let resp = AnthropicResponse {
            id: "msg-tool".to_string(),
            model: "claude-3".to_string(),
            // Mix of text + tool_use. Under stop_reason
            // == "tool_use" the text MUST be dropped.
            content: vec![
                AnthropicContentBlock::Text {
                    text: "I'm calling the weather tool now.".to_string(),
                    cache_control: None,
                },
                AnthropicContentBlock::ToolUse {
                    id: "toolu_1".to_string(),
                    name: "get_weather".to_string(),
                    input: serde_json::json!({"city": "Beijing"}),
                    cache_control: None,
                },
                AnthropicContentBlock::ToolUse {
                    id: "toolu_2".to_string(),
                    name: "get_time".to_string(),
                    input: serde_json::json!({"tz": "UTC"}),
                    cache_control: None,
                },
            ],
            usage: crate::anthropic::types::AnthropicUsage {
                input_tokens: 100,
                output_tokens: 50,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
            },
            stop_reason: Some("tool_use".to_string()),
        };
        let parsed = provider.parse_response(&resp, "claude-3");
        // 2 ToolUse blocks; the Text block was dropped.
        let content = match &parsed.content {
            Content::Multi(parts) => parts.clone(),
            Content::Single(_) => panic!("expected Multi under tool_use"),
        };
        assert_eq!(
            content.len(),
            2,
            "stop_reason=tool_use must produce exactly 2 ToolUse parts; got {:?}",
            content
        );
        // First tool — id/name/input all preserved.
        match &content[0] {
            ContentPart::ToolUse(ToolUse { id, name, input }) => {
                assert_eq!(id, "toolu_1");
                assert_eq!(name, "get_weather");
                assert_eq!(input, &serde_json::json!({"city": "Beijing"}));
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
        match &content[1] {
            ContentPart::ToolUse(ToolUse { id, name, input }) => {
                assert_eq!(id, "toolu_2");
                assert_eq!(name, "get_time");
                assert_eq!(input, &serde_json::json!({"tz": "UTC"}));
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
        // stop_reason MUST propagate to the
        // CompletionResponse (not just be dropped).
        assert_eq!(
            parsed.stop_reason.as_deref(),
            Some("tool_use"),
            "stop_reason must propagate to CompletionResponse.stop_reason"
        );
    }

    /// `parse_response` under `stop_reason != "tool_use"`
    /// (e.g. `"end_turn"`) joins all text blocks
    /// together with `\n` separators and returns
    /// `Content::Single(Text)`. This is the OPPOSITE
    /// contract from the tool_use branch above; without
    /// this test, a refactor that always returns Multi
    /// could silently regress all end_turn responses.
    #[test]
    fn parse_response_with_end_turn_stop_reason_joins_text_into_single() {
        let provider = make_provider();
        let resp = AnthropicResponse {
            id: "msg-end".to_string(),
            model: "claude-3".to_string(),
            // 2 text blocks — must be joined.
            content: vec![
                AnthropicContentBlock::Text {
                    text: "First paragraph.".to_string(),
                    cache_control: None,
                },
                AnthropicContentBlock::Text {
                    text: "Second paragraph.".to_string(),
                    cache_control: None,
                },
            ],
            usage: crate::anthropic::types::AnthropicUsage {
                input_tokens: 5,
                output_tokens: 10,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
            },
            stop_reason: Some("end_turn".to_string()),
        };
        let parsed = provider.parse_response(&resp, "claude-3");
        // Single Text block with both paragraphs joined.
        let txt = match &parsed.content {
            Content::Single(ContentPart::Text(t)) => &t.text,
            Content::Multi(parts) => panic!(
                "expected Single(Text) under end_turn, got Multi with {} parts",
                parts.len()
            ),
            Content::Single(other) => {
                panic!("expected Single(Text), got {other:?}")
            }
        };
        assert_eq!(
            txt, "First paragraph.\nSecond paragraph.",
            "2 text blocks under end_turn must be joined with \\n"
        );
        // stop_reason propagates verbatim.
        assert_eq!(parsed.stop_reason.as_deref(), Some("end_turn"));
    }

    /// Upstream inconsistency guard: when
    /// `stop_reason == "tool_use"` but the content
    /// has NO `tool_use` blocks (the model mentioned
    /// a tool but only emitted text — e.g. some
    /// Claude variants emit "I would call get_weather
    /// but..." then stop), the parser MUST fall back
    /// to joining the text rather than returning an
    /// empty response. Without this fallback, the
    /// agent would loop forever on an empty
    /// `Content::Single(Text(""))`. Pins the
    /// defensive `if tool_uses.is_empty()` branch
    /// in `parse_response`.
    #[test]
    fn parse_response_tool_use_stop_reason_with_no_tool_blocks_falls_back_to_text()
     {
        let provider = make_provider();
        let resp = AnthropicResponse {
            id: "msg-inconsistent".to_string(),
            model: "claude-3".to_string(),
            // Only text blocks — no ToolUse — but the
            // stop_reason says tool_use. This is the
            // upstream inconsistency case.
            content: vec![
                AnthropicContentBlock::Text {
                    text: "I would call get_weather but I forgot the args."
                        .to_string(),
                    cache_control: None,
                },
                AnthropicContentBlock::Text {
                    text: "Sorry, let me just answer instead.".to_string(),
                    cache_control: None,
                },
            ],
            usage: crate::anthropic::types::AnthropicUsage {
                input_tokens: 10,
                output_tokens: 20,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
            },
            stop_reason: Some("tool_use".to_string()),
        };
        let parsed = provider.parse_response(&resp, "claude-3");
        // Falls back to text-join rather than crashing.
        let txt = match &parsed.content {
            Content::Single(ContentPart::Text(t)) => &t.text,
            Content::Multi(parts) => panic!(
                "expected Single(Text) from tool_use→text fallback, got Multi with {} parts",
                parts.len()
            ),
            Content::Single(other) => {
                panic!("expected Single(Text), got {other:?}")
            }
        };
        assert_eq!(
            txt,
            "I would call get_weather but I forgot the args.\nSorry, let me just answer instead.",
            "tool_use stop_reason with zero tool_use blocks MUST fall back to text-join; got {:?}",
            txt
        );
        // stop_reason propagates verbatim — the
        // agent layer can use this to detect the
        // inconsistency if it ever needs to.
        assert_eq!(parsed.stop_reason.as_deref(), Some("tool_use"));
    }

    /// Task 1.6: when an assistant message carries an `assistant`
    /// content with a reasoning block signed by Anthropic, the
    /// request builder must propagate the signature onto the
    /// Anthropic `thinking` block so the API can preserve reasoning
    /// continuity on the next turn.
    #[test]
    fn transform_request_preserves_signature_on_reasoning_block() {
        let provider = make_provider();
        let assistant_msg = Message {
            role: Role::Assistant,
            content: Content::Multi(vec![
                ContentPart::Reasoning(crate::types::ReasoningContent {
                    text: "I'll think this through.".to_string(),
                    signature: Some("sig_persist_for_turn_2".to_string()),
                }),
                ContentPart::Text(TextContent {
                    text: "Here is the answer.".to_string(),
                    cache_control: None,
                }),
            ]),
            tool_call_id: None,
            name: None,
            ..Default::default()
        };
        let req = CompletionRequest {
            model: "claude-3".to_string(),
            messages: Arc::new(vec![
                Message::user("Hello"),
                assistant_msg,
                Message::user("And one more question?"),
            ]),
            tools: Arc::new(vec![]),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
        };

        let anthropic_req = provider.transform_request(&req);
        // Find the assistant message that contained the signed
        // reasoning block.
        let assistant_msg = anthropic_req
            .messages
            .iter()
            .find(|m| m.role == "assistant")
            .expect("assistant message survives transform");
        let thinking_block = assistant_msg
            .content
            .iter()
            .find_map(|b| match b {
                AnthropicContentBlock::ThinkingBlock {
                    thinking,
                    signature,
                } if signature.is_some() => {
                    Some((thinking.clone(), signature.clone()))
                }
                _ => None,
            })
            .expect("thinking block with signature survives transform");
        assert_eq!(thinking_block.0, "I'll think this through.");
        assert_eq!(
            thinking_block.1.as_deref(),
            Some("sig_persist_for_turn_2"),
            "signature must propagate to the Anthropic wire form"
        );
    }

    /// `with_api_key(key)` MUST store the key, and
    /// `make_request` MUST emit an `x-api-key: <key>`
    /// header when an api key is configured. Without
    /// this pin, a refactor that removes the
    /// conditional header would silently produce
    /// 401s in production.
    ///
    /// We can't easily inspect a `reqwest::RequestBuilder`
    /// without firing the request, but the key
    /// stored in the struct is the observable signal:
    /// we verify the constructor and accessor.
    #[test]
    fn with_api_key_stores_key_in_struct() {
        let p = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        })
        .with_api_key("sk-test-12345");
        assert_eq!(
            p.api_key.as_deref(),
            Some("sk-test-12345"),
            "with_api_key must store the key verbatim"
        );
    }

    /// Default constructor MUST leave `api_key` as
    /// `None` so an unconfigured provider cannot
    /// accidentally send an empty `x-api-key: ""`
    /// header (which Anthropic treats as a 401).
    #[test]
    fn default_constructor_has_no_api_key() {
        let p = AnthropicProvider::new(ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        });
        assert!(
            p.api_key.is_none(),
            "default-constructed provider must have api_key=None; got {:?}",
            p.api_key
        );
    }

    /// `with_base_url` MUST strip a single trailing
    /// `/` (otherwise `format!("{}/v1/messages", base_url)`
    /// yields a double-slash URL). Pin the exact
    /// transformation: only ONE trailing `/` is
    /// stripped (idempotency), and a base URL
    /// without a trailing `/` is left untouched.
    #[test]
    fn with_base_url_strips_a_single_trailing_slash() {
        let cfg = ModelConfig {
            name: "claude-3".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        };
        let p1 = AnthropicProvider::new(cfg.clone())
            .with_base_url("https://api.example.com/");
        assert_eq!(p1.base_url, "https://api.example.com");
        let p2 = AnthropicProvider::new(cfg.clone())
            .with_base_url("https://api.example.com");
        assert_eq!(
            p2.base_url, "https://api.example.com",
            "no trailing slash must be left untouched"
        );
        // Idempotency: stripping an already-stripped URL
        // is a no-op.
        let p3 = AnthropicProvider::new(cfg).with_base_url(&p1.base_url);
        assert_eq!(p3.base_url, "https://api.example.com");
    }

    /// `make_request` is `async` and returns a
    /// `reqwest::RequestBuilder` rather than a
    /// `Response` — we cannot run the actual HTTP
    /// request without a server, but we CAN verify
    /// the function:
    ///   - returns `Ok(_)` for a well-formed request
    ///   - returns `Err` if the transform step
    ///     produces a body that fails to serialize
    /// Pin the success path against a future change
    /// that turns the `unwrap_or_default()` into
    /// a panicking `unwrap()` on serialization
    /// failure.
    #[tokio::test]
    async fn make_request_succeeds_for_well_formed_request() {
        let provider = make_provider();
        let req = make_request_no_policy();
        let builder = provider
            .make_request(&req)
            .await
            .expect("well-formed request must succeed");
        // Smoke check — the builder is non-empty.
        // We can't introspect headers without firing
        // the request, but the Ok return is the
        // primary contract.
        drop(builder);
    }
}
