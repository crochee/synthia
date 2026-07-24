//! Anthropic SSE stream processor.
//!
//! Two processors live here:
//!
//! - [`legacy::StreamProcessor`] — the legacy processor used by
//!   the deprecated `ModelProvider::stream()` method. It emits
//!   `Content(ContentPart::ToolUse)` with the full accumulated
//!   input on every delta.
//! - [`v2::StreamProcessorV2`] — the new processor used by
//!   `complete_with_stream`. It emits incremental
//!   `ToolCallStart { id, name, arguments }`,
//!   `ToolCallDelta { id, arguments_delta }`, `ToolCallEnd { id }`,
//!   and a terminal `IsDone { result: SamplingResult }` on
//!   `message_stop`.
//!
//! # Module Layout
//!
//! - [`events`]: The 3 raw SSE event structs
//!   ([`events::AnthropicStreamEvent`],
//!   [`events::AnthropicStreamContentBlock`],
//!   [`events::AnthropicStreamDelta`]). Used by both
//!   `legacy` and `v2` processors.
//! - [`legacy`]: The legacy [`legacy::StreamProcessor`] +
//!   its private [`legacy::ToolUseBuffer`].
//! - [`v2`]: The new [`v2::StreamProcessorV2`] + its private
//!   [`v2::V2ToolUseBuffer`] + 2 free helpers
//!   ([`v2::parse_tool_input`],
//!   [`v2::delta_usage`]).
//! - [`tests`]: All 5 unit tests covering
//!   `text_delta` → `Content(Text)`, `thinking_delta` →
//!   `Content(Reasoning)`, the full `ToolUse` start/delta/end +
//!   `IsDone` round-trip with parsed JSON, `IsDone` carries
//!   accumulated text, and `parse_tool_input` edge cases
//!   (empty string → empty object, invalid JSON → raw string).

mod events;
mod legacy;
mod v2;

#[cfg(test)]
mod tests;

pub use events::{
    AnthropicStreamContentBlock,
    AnthropicStreamDelta,
    AnthropicStreamEvent,
};
pub use legacy::StreamProcessor;
pub use v2::StreamProcessorV2;
