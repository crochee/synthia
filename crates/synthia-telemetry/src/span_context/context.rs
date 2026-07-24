use super::types::SpanAttributes;

/// Manages the span hierarchy for an agent session.
///
/// The hierarchy is:
///   session_root (covers entire session lifetime)
///     └── invocation (covers a single user input / ReAct loop)
///           ├── llm_call
///           ├── tool_execution
///           ├── context_assembly
///           ├── guardian_check
///           └── compaction
///
/// Spans are created using the tracing crate. Entering a span makes it the
/// implicit parent for any spans created within its scope.
#[derive(Clone)]
pub struct SpanContext {
    session_id: String,
    session_span: tracing::Span,
}

impl SpanContext {
    /// Create a new SpanContext for the given session.
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            session_span: tracing::Span::none(),
        }
    }

    /// Start the session root span. Call this at the beginning of a session.
    /// Returns a guard that keeps the session span active.
    pub fn session_start(&mut self) -> tracing::span::EnteredSpan {
        self.session_span = tracing::info_span!(
            "session",
            session_id = %self.session_id,
            span.kind = "session_root"
        );
        self.session_span.clone().entered()
    }

    /// Start an invocation span (child of session). Call this once per user input.
    /// Returns a guard that keeps the invocation span active.
    pub fn invocation_start(
        &self,
        iteration: usize,
    ) -> tracing::span::EnteredSpan {
        tracing::info_span!(
            "invocation",
            session_id = %self.session_id,
            iteration_number = iteration,
            span.kind = "invocation"
        )
        .entered()
    }

    /// Create a step span for an LLM call (child of current span context).
    pub fn step_llm_call(
        &self,
        iteration: usize,
        model: &str,
    ) -> tracing::Span {
        tracing::info_span!(
            "llm_call",
            session_id = %self.session_id,
            iteration_number = iteration,
            model = model,
            span.kind = "llm_call"
        )
    }

    /// Create a step span for an LLM call with full attributes.
    pub fn step_llm_call_with_attrs(
        &self,
        iteration: usize,
        model: &str,
        prefix_hash: &str,
        tokens_in: usize,
        tokens_out: usize,
        latency_ms: u64,
    ) -> tracing::Span {
        tracing::info_span!(
            "llm_call",
            session_id = %self.session_id,
            iteration_number = iteration,
            model = model,
            prefix_hash = prefix_hash,
            tokens_in = tokens_in,
            tokens_out = tokens_out,
            latency_ms = latency_ms,
            span.kind = "llm_call"
        )
    }

    /// Create a step span for a tool execution (child of current span context).
    pub fn step_tool_execution(
        &self,
        iteration: usize,
        tool_name: &str,
        tool_call_id: &str,
    ) -> tracing::Span {
        tracing::info_span!(
            "tool_execution",
            session_id = %self.session_id,
            iteration_number = iteration,
            tool_name = tool_name,
            tool_call_id = tool_call_id,
            span.kind = "tool_execution"
        )
    }

    /// Create a step span for context assembly.
    pub fn step_context_assembly(
        &self,
        iteration: usize,
        token_count: usize,
    ) -> tracing::Span {
        tracing::info_span!(
            "context_assembly",
            session_id = %self.session_id,
            iteration_number = iteration,
            token_count = token_count,
            span.kind = "context_assembly"
        )
    }

    /// Create a step span for guardian check.
    pub fn step_guardian_check(&self, iteration: usize) -> tracing::Span {
        tracing::info_span!(
            "guardian_check",
            session_id = %self.session_id,
            iteration_number = iteration,
            span.kind = "guardian_check"
        )
    }

    /// Create a step span for compaction.
    pub fn step_compaction(
        &self,
        iteration: usize,
        old_tokens: usize,
        new_tokens: usize,
    ) -> tracing::Span {
        tracing::info_span!(
            "compaction",
            session_id = %self.session_id,
            iteration_number = iteration,
            old_tokens = old_tokens,
            new_tokens = new_tokens,
            span.kind = "compaction"
        )
    }

    /// Create a generic step span with custom attributes.
    pub fn step(
        &self,
        name: &str,
        iteration: usize,
        attrs: SpanAttributes,
    ) -> tracing::Span {
        let span = tracing::info_span!(
            "{}",
            name,
            session_id = %self.session_id,
            iteration_number = iteration,
        );

        // Record additional attributes
        let _enter = span.enter();
        for (key, value) in &attrs {
            tracing::Span::current().record(
                key.as_str(),
                serde_json::to_string(value).unwrap_or_default(),
            );
        }

        span.clone()
    }

    /// Get the session_id.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}
