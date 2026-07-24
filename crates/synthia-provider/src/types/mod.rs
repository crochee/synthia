//! Canonical provider-agnostic LLM message / tool / streaming types.
//!
//! This module defines the wire-format that flows from
//! `synthia-agent` through `synthia-context` (prompt assembly /
//! truncation) into the LLM providers. The types are deliberately
//! provider-agnostic — every `Provider` adapter translates them
//! to its own wire format (`openai` / `anthropic`).
//!
//! # Module Layout
//!
//! - [`role`]: The [`role::Role`] enum (System/User/Assistant/Tool).
//! - [`message`]: The [`message::Message`] struct + Default + 5
//!   constructors ([`message::Message::new`],
//!   [`message::Message::user`],
//!   [`message::Message::assistant`],
//!   [`message::Message::system`],
//!   [`message::Message::tool`]).
//! - [`content`]: The [`content::Content`] enum + the
//!   [`content::ContentPart`] variant data + all
//!   `From<String>` / `From<&str>` / `IntoIterator` impls +
//!   the [`content::ContentPart::is_tool_use`] /
//!   [`content::ContentPart::text`] accessors.
//! - [`completion`]: [`completion::CompletionRequest`] +
//!   [`completion::CompletionResponse`] +
//!   [`completion::ToolChoice`].
//! - [`tool`]: [`tool::ToolUse`] + [`tool::ToolResult`] +
//!   [`tool::ToolDefinition`] + [`tool::ResourceLink`].
//! - [`stream_chunk`]: [`stream_chunk::StreamChunk`] +
//!   [`stream_chunk::SamplingResult`] + 2 `From` impls.
//! - [`models`]: [`models::TokenUsage`] +
//!   [`models::ModelInfo`] + [`models::ProviderInfo`] +
//!   [`models::ProviderConfig`] + [`models::ModelConfig`].
//! - [`tests`]: All unit tests (4 in `stream_chunk_tests` +
//!   4 in `tool_result_cleared_at_tests`).

mod completion;
mod content;
mod message;
mod models;
mod role;
mod stream_chunk;
mod tool;

#[cfg(test)]
mod tests;

pub use completion::{CompletionRequest, CompletionResponse, ToolChoice};
pub use content::{
    AudioContent,
    AudioFormat,
    Content,
    ContentPart,
    ImageContent,
    ImageDetail,
    TextContent,
};
pub use message::Message;
pub use models::{
    ModelConfig,
    ModelInfo,
    ProviderConfig,
    ProviderInfo,
    TokenUsage,
};
pub use role::Role;
pub use stream_chunk::{SamplingResult, StreamChunk};
pub use tool::{ResourceLink, ToolDefinition, ToolResult, ToolUse};
