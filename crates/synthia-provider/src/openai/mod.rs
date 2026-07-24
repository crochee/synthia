//! OpenAI-compatible provider implementation.
//!
//! Submodule layout:
//!
//! - [`types`]: the wire types sent to / received from
//!   `POST /chat/completions` and `POST /embeddings`
//!   (`OpenAIRequest`, `OpenAIMessage`, `OpenAIContentPart`,
//!   `OpenAIResponse`, `OpenAIEmbeddingRequest`, etc.) plus the
//!   `serialize_content` / `deserialize_content` adapters.
//! - [`provider`]: the `OpenAICompatibleProvider` struct itself,
//!   its constructors, and the request/response transformation
//!   helpers (`transform_request`, `transform_message*`,
//!   `transform_content`, `transform_part`, `parse_response`,
//!   `make_request`, `TransformOptions`).
//! - [`traits_impl`]: `impl ModelProvider for OpenAICompatibleProvider`
//!   (`initialize`, `complete`, `complete_with_stream`, `embed`)
//!   and the `wait_cancel_openai` `tokio::select!` arm helper.
//! - [`token`]: `estimate_tokens` and `impl TokenCounter for
//!   OpenAICompatibleProvider`.

mod provider;
mod token;
mod traits_impl;
mod types;

pub use provider::{OpenAICompatibleProvider, TransformOptions};
pub use token::estimate_tokens;
