//! Unified streaming module for all providers
//!
//! This module consolidates streaming functionality from multiple providers:
//! - Anthropic SSE event processing (`anthropic`, feature `anthropic`)
//! - Stream response collection utilities
//! - Shared `<think>…</think>` extraction for non-native reasoning providers
//! - The shared SSE pump with the per-read idle watchdog (dsh
//!   `idleWatchdog` parity) both adapters' `complete_with_stream` run on
//!
//! OpenAI streaming lives in `crate::openai_streaming` (private to the
//! provider crate, used only by `OpenAICompatibleProvider`) but reuses
//! the shared `ThinkExtractor` and `pump_sse` from here.
//!
//! The adapter-specific pieces are gated with the adapter's feature;
//! the shared extractor and watchdog compile either way.

#[cfg(feature = "anthropic")]
mod anthropic;
mod idle_watchdog;
mod think_extractor;
mod tool_args;

#[cfg(feature = "anthropic")]
pub use anthropic::{
    AnthropicStreamContentBlock,
    AnthropicStreamDelta,
    AnthropicStreamEvent,
    StreamProcessor,
};
#[cfg(any(feature = "anthropic", feature = "openai"))]
pub(crate) use idle_watchdog::pump_sse;
pub use idle_watchdog::{
    DEFAULT_STREAM_IDLE_TIMEOUT,
    IDLE_TIMEOUT_MARKER,
    idle_timeout_error,
    is_idle_timeout,
};
pub use think_extractor::ThinkExtractor;
pub use tool_args::{ToolUseBuffer, ToolUseBufferMap, parse_tool_input};
