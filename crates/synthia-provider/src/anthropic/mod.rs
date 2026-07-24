//! Anthropic Messages API provider implementation.
//!
//! Submodule layout:
//!
//! - [`types`]: the wire types sent to / received from
//!   `POST /v1/messages` (`AnthropicRequest`, `AnthropicMessage`,
//!   `AnthropicContentBlock`, `AnthropicResponse`, etc.) plus the
//!   `deserialize_tool_result_content` adapter.
//! - [`provider`]: the `AnthropicProvider` struct itself, its
//!   constructors, and the request/response transformation helpers
//!   (`transform_request`, `transform_message`, `transform_part`,
//!   `reorder_anthropic_messages`, `sanitize_tool_id`,
//!   `parse_response`, `make_request`).
//! - [`traits_impl`]: `impl ModelProvider for AnthropicProvider`
//!   (`initialize`, `complete`, `complete_with_stream`, `embed`)
//!   and the `wait_cancel` `tokio::select!` arm helper.
//! - [`token`]: `estimate_tokens` and `impl TokenCounter for
//!   AnthropicProvider`.

mod provider;
mod token;
mod traits_impl;
mod types;

pub use provider::AnthropicProvider;
pub use token::estimate_tokens;
