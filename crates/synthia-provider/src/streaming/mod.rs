//! Unified streaming module for all providers
//!
//! This module consolidates streaming functionality from multiple providers:
//! - Anthropic SSE event processing
//! - Stream response collection utilities
//!
//! OpenAI streaming lives in `crate::openai_streaming` (private to the
//! provider crate, used only by `OpenAICompatibleProvider`).

mod anthropic;

pub use anthropic::{
    AnthropicStreamContentBlock,
    AnthropicStreamDelta,
    AnthropicStreamEvent,
    StreamProcessor,
    StreamProcessorV2,
};
