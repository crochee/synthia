//! Unified streaming module for all providers
//!
//! This module consolidates streaming functionality from multiple providers:
//! - Anthropic SSE event processing
//! - Stream response collection utilities
//! - Shared `<think>…</think>` extraction for non-native reasoning providers
//!
//! OpenAI streaming lives in `crate::openai_streaming` (private to the
//! provider crate, used only by `OpenAICompatibleProvider`) but reuses
//! the shared `ThinkExtractor` from here.

mod anthropic;
mod think_extractor;
mod tool_args;

pub use anthropic::{
    AnthropicStreamContentBlock,
    AnthropicStreamDelta,
    AnthropicStreamEvent,
    StreamProcessor,
};
pub use think_extractor::ThinkExtractor;
pub use tool_args::{ToolUseBuffer, ToolUseBufferMap, parse_tool_input};
