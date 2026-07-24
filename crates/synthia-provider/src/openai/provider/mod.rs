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
