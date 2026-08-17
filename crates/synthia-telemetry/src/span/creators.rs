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

#[cfg(test)]
mod tests {
    use super::*;

    /// Root session span MUST carry the
    /// session_id attribute and have no
    /// parent_span_id (empty string).
    #[test]
    fn session_span_root_carries_session_id_with_no_parent() {
        let (ctx, _guard) = create_session_span("sess-1");
        assert_eq!(
            ctx.attributes.get("session_id"),
            Some(&"sess-1".to_string())
        );
        // No parent for the root span.
        assert!(ctx.parent_span_id.is_empty());
        // Trace ID is freshly generated.
        assert!(!ctx.trace_id.is_empty());
    }

    /// Invocation span MUST inherit
    /// session_id from its parent + stamp
    /// `invocation_id` and `iteration_number`.
    #[test]
    fn invocation_span_inherits_session_id_and_stamps_iteration() {
        let (parent, _guard) = create_session_span("sess-7");
        let (ctx, _) = create_invocation_span(&parent, "inv-3", 5);
        assert_eq!(
            ctx.attributes.get("session_id"),
            Some(&"sess-7".to_string())
        );
        assert_eq!(
            ctx.attributes.get("invocation_id"),
            Some(&"inv-3".to_string())
        );
        assert_eq!(
            ctx.attributes.get("iteration_number"),
            Some(&"5".to_string())
        );
        // Parent linkage.
        assert_eq!(ctx.parent_span_id, parent.span_id);
    }

    /// Step span (LlmCall/ToolExecution/etc.)
    /// MUST inherit session_id from parent +
    /// stamp iteration_number.
    #[test]
    fn step_span_inherits_session_id_and_stamps_iteration() {
        let (parent, _guard) = create_session_span("sess-9");
        let (ctx, _) =
            create_step_span(&parent, SpanKind::LlmCall, "llm_call", 2);
        assert_eq!(
            ctx.attributes.get("session_id"),
            Some(&"sess-9".to_string())
        );
        assert_eq!(
            ctx.attributes.get("iteration_number"),
            Some(&"2".to_string())
        );
    }

    /// LLM call span MUST stamp the 5
    /// standard attributes: model,
    /// prefix_hash, tokens_in, tokens_out,
    /// latency_ms.
    #[test]
    fn llm_call_span_stamps_all_required_attributes() {
        let (parent, _guard) = create_session_span("sess-1");
        let (ctx, _) = create_llm_call_span(
            &parent,
            1,
            "claude-opus-4-7",
            "abc123",
            100,
            50,
            250,
        );
        assert_eq!(
            ctx.attributes.get("model"),
            Some(&"claude-opus-4-7".to_string())
        );
        assert_eq!(
            ctx.attributes.get("prefix_hash"),
            Some(&"abc123".to_string())
        );
        assert_eq!(ctx.attributes.get("tokens_in"), Some(&"100".to_string()));
        assert_eq!(ctx.attributes.get("tokens_out"), Some(&"50".to_string()));
        assert_eq!(ctx.attributes.get("latency_ms"), Some(&"250".to_string()));
    }

    /// Tool execution span MUST stamp
    /// tool_name, tool_call_id.
    #[test]
    fn tool_execution_span_stamps_all_required_attributes() {
        let (parent, _guard) = create_session_span("sess-1");
        let (ctx, _) = create_tool_execution_span(&parent, 1, "bash", "call-1");
        assert_eq!(ctx.attributes.get("tool_name"), Some(&"bash".to_string()));
        assert_eq!(
            ctx.attributes.get("tool_call_id"),
            Some(&"call-1".to_string())
        );
    }

    /// Context assembly span MUST stamp
    /// token_count.
    #[test]
    fn context_assembly_span_stamps_token_count() {
        let (parent, _guard) = create_session_span("sess-1");
        let (ctx, _) = create_context_assembly_span(&parent, 3, 800);
        assert_eq!(ctx.attributes.get("token_count"), Some(&"800".to_string()));
    }

    /// Guardian check span MUST stamp
    /// iteration_number only.
    #[test]
    fn guardian_check_span_stamps_iteration_only() {
        let (parent, _guard) = create_session_span("sess-1");
        let (ctx, _) = create_guardian_check_span(&parent, 7);
        assert_eq!(
            ctx.attributes.get("iteration_number"),
            Some(&"7".to_string())
        );
    }

    /// Compaction span MUST stamp
    /// old_tokens, new_tokens.
    #[test]
    fn compaction_span_stamps_old_and_new_token_counts() {
        let (parent, _guard) = create_session_span("sess-1");
        let (ctx, _) = create_compaction_span(&parent, 1, 5000, 2000);
        assert_eq!(ctx.attributes.get("old_tokens"), Some(&"5000".to_string()));
        assert_eq!(ctx.attributes.get("new_tokens"), Some(&"2000".to_string()));
    }

    /// When a child span is created with a
    /// parent that has no `session_id`
    /// attribute, the child MUST still
    /// carry an empty (but present)
    /// `session_id` attribute rather than
    /// omit it entirely.
    #[test]
    fn step_span_with_parent_missing_session_id_carries_empty_session_id() {
        let parent = SpanContext::root("trace-1");
        // Intentionally do NOT set session_id
        // on parent.
        let (ctx, _) =
            create_step_span(&parent, SpanKind::LlmCall, "llm_call", 0);
        assert!(
            ctx.attributes.contains_key("session_id"),
            "session_id attribute MUST always be present (even if empty)"
        );
        assert_eq!(ctx.attributes.get("session_id"), Some(&String::new()));
    }
}
