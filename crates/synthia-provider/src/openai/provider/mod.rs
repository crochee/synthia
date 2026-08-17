//! `OpenAICompatibleProvider` — the stateful struct + its
//! request/response transformation helpers. The
//! `ModelProvider` trait impl lives in `traits_impl`.
//!
//! # Module Layout
//!
//! - `types`: [`types::TransformOptions`] struct (a
//!   placeholder for future options).
//! - `core`: [`core::OpenAICompatibleProvider`] struct +
//!   `new` / `with_api_key` constructors.
//! - `transform`: 3 transform methods
//!   ([`transform::OpenAICompatibleProvider::transform_request`],
//!   `transform_message`, `transform_message_with_options`).
//! - `tool_message`:
//!   [`tool_message::OpenAICompatibleProvider::transform_tool_message`].
//! - `content`: 2 content methods
//!   ([`content::OpenAICompatibleProvider::transform_content`],
//!   `transform_part`).
//! - `response`:
//!   [`response::OpenAICompatibleProvider::parse_response`].
//! - `request`:
//!   [`request::OpenAICompatibleProvider::make_request`].

mod content;
mod core;
mod request;
mod response;
mod tool_message;
mod transform;
mod types;

pub use core::OpenAICompatibleProvider;

pub use types::TransformOptions;

#[cfg(test)]
mod tests {
    //! Constructor + request-builder edge cases for
    //! [`OpenAICompatibleProvider`]. The pre-existing
    //! `transform` / `response` unit tests cover
    //! message-level parsing, but the constructor
    //! flow and `make_request` header/URL
    //! composition have no coverage. This module
    //! pins:
    //!
    //! - `with_api_key(key)` stores the key verbatim
    //!   so `make_request` can attach `Authorization:
    //!   Bearer <key>`.
    //! - Default `api_key` is `None` (no
    //!   `Authorization: Bearer ""` header — which
    //!   some OpenAI-compatible gateways reject
    //!   outright).
    //! - `make_request` succeeds for a well-formed
    //!   request (no panic in
    //!   `serde_json::to_string(&body).unwrap_or_default()`).
    use super::OpenAICompatibleProvider;
    use crate::ModelConfig;

    fn cfg() -> ModelConfig {
        ModelConfig {
            name: "gpt-4".to_string(),
            provider: "openai".to_string(),
            context_window: 8192,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        }
    }

    #[test]
    fn with_api_key_stores_key_in_struct() {
        let p = OpenAICompatibleProvider::new(
            "https://api.openai.com".to_string(),
            cfg(),
        )
        .with_api_key("sk-openai-test-12345");
        assert_eq!(
            p.api_key.as_deref(),
            Some("sk-openai-test-12345"),
            "with_api_key must store the key verbatim"
        );
    }

    #[test]
    fn default_constructor_has_no_api_key() {
        let p = OpenAICompatibleProvider::new(
            "https://api.openai.com".to_string(),
            cfg(),
        );
        assert!(
            p.api_key.is_none(),
            "default-constructed provider must have api_key=None; got {:?}",
            p.api_key
        );
    }

    #[test]
    fn base_url_is_stored_verbatim() {
        // Unlike the Anthropic provider, the OpenAI
        // provider does NOT strip a trailing slash
        // — `with_base_url` is not a method. The
        // caller is responsible for the URL format.
        // Pin the verbatim-storage contract so a
        // future refactor that adds stripping does
        // so intentionally.
        let p1 = OpenAICompatibleProvider::new(
            "https://api.example.com/v1".to_string(),
            cfg(),
        );
        assert_eq!(p1.base_url, "https://api.example.com/v1");
        let p2 = OpenAICompatibleProvider::new(
            "https://api.example.com/v1/".to_string(),
            cfg(),
        );
        assert_eq!(
            p2.base_url, "https://api.example.com/v1/",
            "OpenAI provider does NOT strip trailing slash — caller must"
        );
    }

    #[tokio::test]
    async fn make_request_succeeds_for_well_formed_request() {
        use std::sync::Arc;

        use crate::types::{
            CompletionRequest,
            Content,
            ContentPart,
            Message,
            Role,
            TextContent,
            ToolChoice,
        };
        let provider = OpenAICompatibleProvider::new(
            "https://api.openai.com/v1".to_string(),
            cfg(),
        );
        let req = CompletionRequest {
            model: "gpt-4".to_string(),
            messages: Arc::new(vec![Message {
                role: Role::User,
                content: Content::Single(ContentPart::Text(TextContent {
                    text: "hi".to_string(),
                    cache_control: None,
                })),
                tool_call_id: None,
                name: None,
                tool_result_cleared_at: None,
            }]),
            tools: Arc::new(vec![]),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
        };
        let builder = provider
            .make_request(&req)
            .await
            .expect("well-formed request must succeed");
        drop(builder);
    }
}
