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

    use synthia_cache_mark::{CacheControlMark, CacheScope, CacheTtl};

    use crate::{
        anthropic::{
            AnthropicProvider,
            types::{AnthropicContentBlock, AnthropicResponse, AnthropicUsage},
        },
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
                text: "hi".to_string(),
                cache_control: None,
            }],
            usage: AnthropicUsage {
                input_tokens: 10,
                output_tokens: 5,
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
}
