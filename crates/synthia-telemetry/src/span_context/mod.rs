//! Span hierarchy management for agent sessions.
//!
//! Provides a structured way to create and manage OpenTelemetry-compatible
//! span hierarchies: session -> invocation -> steps (llm_call, tool_execution,
//! context_assembly, guardian_check, compaction).
//!
//! # Usage
//!
//! ```ignore
//! let span_ctx = SpanContext::new("session-1");
//! span_ctx.session_start();
//!
//! let _invocation = span_ctx.invocation_start(1);
//!
//! // Each step creates a child span of the invocation
//! let _step = span_ctx.step_llm_call(1, "gpt-4");
//! ```

mod context;
mod types;

pub use context::SpanContext;
pub use types::{SpanAttributes, StepKind};

#[cfg(test)]
mod tests;
