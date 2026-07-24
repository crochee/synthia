//! OpenTelemetry span hierarchy for agent sessions.
//!
//! Provides structured span creation with proper parent-child relationships:
//!   session (root)
//!     └── invocation (per user input)
//!           ├── llm_call
//!           ├── tool_execution
//!           ├── context_assembly
//!           ├── guardian_check
//!           └── compaction
//!
//! # Usage
//!
//! ```ignore
//! let session = create_session_span("session-123");
//! let invocation = create_invocation_span(&session, "inv-1");
//! let llm = create_step_span(&invocation, SpanKind::LlmCall, "llm_call");
//! ```
//!
//! The original 694-line `span.rs` was split into focused
//! submodules by responsibility:
//!
//! - `kind`: the 7-variant [`SpanKind`] enum +
//!   the `name()` method.
//! - `context`: the [`SpanContext`] data carrier +
//!   lifecycle methods (`root` / `child` / `end` /
//!   `with_attribute` / `set_attribute` / `duration`).
//! - `builder`: the [`SpanBuilder`] orchestrator that
//!   produces `(SpanContext, tracing::Span)` pairs.
//! - `creators`: the 7 `create_*_span` convenience
//!   functions that wrap [`SpanBuilder`] for the
//!   common hierarchy cases.
//!
//! The 22 unit tests live in `tests`.

mod builder;
mod context;
mod creators;
mod kind;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

#[cfg(feature = "otel")]
pub mod attributes_processor;

#[cfg(feature = "otel")]
pub use attributes_processor::SpanAttributesProcessor;
pub use builder::SpanBuilder;
pub use context::SpanContext;
pub use creators::{
    create_compaction_span,
    create_context_assembly_span,
    create_guardian_check_span,
    create_invocation_span,
    create_llm_call_span,
    create_session_span,
    create_step_span,
    create_tool_execution_span,
};
pub use kind::SpanKind;
