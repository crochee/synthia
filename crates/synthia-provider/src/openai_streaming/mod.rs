//! OpenAI-compatible stream processor.
//!
//! This module implements the chunk model required by
//! `complete_with_stream`:
//! - `Content(Text)` for `delta.content`
//! - `Content(Reasoning)` for `delta.reasoning_content` (no sniffing)
//! - `ToolCallStart { id, name, arguments }` on the first delta for a tool call
//! - `ToolCallDelta { id, arguments_delta }` on subsequent argument deltas
//! - `ToolCallEnd { id }` on the delta where the tool call receives
//!   its last argument
//! - `IsDone { result: SamplingResult }` on the delta with `finish_reason`

mod processor;
#[cfg(test)]
mod tests;
mod types;

pub use processor::OpenAIStreamProcessor;
