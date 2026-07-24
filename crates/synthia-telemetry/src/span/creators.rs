//! `create_*_span` convenience functions.
//!
//! Seven one-shot factory functions that wrap
//! [`super::builder::SpanBuilder`] for the common
//! hierarchy cases. All seven follow the same
//! shape: pick a [`super::kind::SpanKind`], attach
//! the parent (`None` only for
//! [`create_session_span`]), stamp the standard
//! `iteration_number` / `session_id` attributes,
//! stamp the kind-specific attributes, and `build()`.
//!
//! The `session_id` is propagated from the parent
//! span's attributes into every child span's
//! attributes — this is the only piece of state
//! that crosses span boundaries (so tracing queries
//! like "show me all spans for session `sess-7`"
//! work without walking the parent chain at query
//! time).
//!
//! Kept separate from [`super::builder`] (the
//! generic builder) and [`super::context`] (the
//! data carrier) so the seven public entry points
//! all live in one readable file.

use super::{builder::SpanBuilder, context::SpanContext, kind::SpanKind};

/// Create a session root span covering the entire session lifetime.
/// Returns (SpanContext, tracing::Span).
pub fn create_session_span(session_id: &str) -> (SpanContext, tracing::Span) {
    let trace_id = uuid::Uuid::new_v4().to_string();

    SpanBuilder::new(SpanKind::Session, "session")
        .with_trace_id(&trace_id)
        .with_attribute("session_id", session_id)
        .build()
}

/// Create an invocation span as a child of the session span.
/// Covers a single user input / ReAct loop iteration.
pub fn create_invocation_span(
    parent: &SpanContext,
    invocation_id: &str,
    iteration: usize,
) -> (SpanContext, tracing::Span) {
    SpanBuilder::new(SpanKind::Invocation, "invocation")
        .with_parent(parent)
        .with_attribute("invocation_id", invocation_id)
        .with_attribute("iteration_number", &iteration.to_string())
        .with_attribute(
            "session_id",
            &parent
                .attributes
                .get("session_id")
                .cloned()
                .unwrap_or_default(),
        )
        .build()
}

/// Create a step span as a child of the invocation span.
/// Supported kinds: LlmCall, ToolExecution, ContextAssembly, GuardianCheck, Compaction.
pub fn create_step_span(
    parent: &SpanContext,
    kind: SpanKind,
    name: &str,
    iteration: usize,
) -> (SpanContext, tracing::Span) {
    SpanBuilder::new(kind, name)
        .with_parent(parent)
        .with_attribute("iteration_number", &iteration.to_string())
        .with_attribute(
            "session_id",
            &parent
                .attributes
                .get("session_id")
                .cloned()
                .unwrap_or_default(),
        )
        .build()
}

/// Create an LLM call span with all required attributes.
pub fn create_llm_call_span(
    parent: &SpanContext,
    iteration: usize,
    model: &str,
    prefix_hash: &str,
    tokens_in: usize,
    tokens_out: usize,
    latency_ms: u64,
) -> (SpanContext, tracing::Span) {
    SpanBuilder::new(SpanKind::LlmCall, "llm_call")
        .with_parent(parent)
        .with_attribute("iteration_number", &iteration.to_string())
        .with_attribute("model", model)
        .with_attribute("prefix_hash", prefix_hash)
        .with_attribute("tokens_in", &tokens_in.to_string())
        .with_attribute("tokens_out", &tokens_out.to_string())
        .with_attribute("latency_ms", &latency_ms.to_string())
        .with_attribute(
            "session_id",
            &parent
                .attributes
                .get("session_id")
                .cloned()
                .unwrap_or_default(),
        )
        .build()
}

/// Create a tool execution span with attributes.
pub fn create_tool_execution_span(
    parent: &SpanContext,
    iteration: usize,
    tool_name: &str,
    tool_call_id: &str,
) -> (SpanContext, tracing::Span) {
    SpanBuilder::new(SpanKind::ToolExecution, "tool_execution")
        .with_parent(parent)
        .with_attribute("iteration_number", &iteration.to_string())
        .with_attribute("tool_name", tool_name)
        .with_attribute("tool_call_id", tool_call_id)
        .with_attribute(
            "session_id",
            &parent
                .attributes
                .get("session_id")
                .cloned()
                .unwrap_or_default(),
        )
        .build()
}

/// Create a context assembly span.
pub fn create_context_assembly_span(
    parent: &SpanContext,
    iteration: usize,
    token_count: usize,
) -> (SpanContext, tracing::Span) {
    SpanBuilder::new(SpanKind::ContextAssembly, "context_assembly")
        .with_parent(parent)
        .with_attribute("iteration_number", &iteration.to_string())
        .with_attribute("token_count", &token_count.to_string())
        .with_attribute(
            "session_id",
            &parent
                .attributes
                .get("session_id")
                .cloned()
                .unwrap_or_default(),
        )
        .build()
}

/// Create a guardian check span.
pub fn create_guardian_check_span(
    parent: &SpanContext,
    iteration: usize,
) -> (SpanContext, tracing::Span) {
    SpanBuilder::new(SpanKind::GuardianCheck, "guardian_check")
        .with_parent(parent)
        .with_attribute("iteration_number", &iteration.to_string())
        .with_attribute(
            "session_id",
            &parent
                .attributes
                .get("session_id")
                .cloned()
                .unwrap_or_default(),
        )
        .build()
}

/// Create a compaction span.
pub fn create_compaction_span(
    parent: &SpanContext,
    iteration: usize,
    old_tokens: usize,
    new_tokens: usize,
) -> (SpanContext, tracing::Span) {
    SpanBuilder::new(SpanKind::Compaction, "compaction")
        .with_parent(parent)
        .with_attribute("iteration_number", &iteration.to_string())
        .with_attribute("old_tokens", &old_tokens.to_string())
        .with_attribute("new_tokens", &new_tokens.to_string())
        .with_attribute(
            "session_id",
            &parent
                .attributes
                .get("session_id")
                .cloned()
                .unwrap_or_default(),
        )
        .build()
}
