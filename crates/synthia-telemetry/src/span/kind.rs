//! Span kind taxonomy.
//!
//! The 7-variant [`SpanKind`] enum is the discriminator
//! for every span this module creates. Each variant
//! maps to a stable string name via [`SpanKind::name`]
//! — that name is what shows up in the
//! `tracing::info_span!` call inside
//! [`super::builder::SpanBuilder::build_tracing_span`]
//! and in OTel export tags.
//!
//! Variant / name map:
//!
//! | Variant             | Name               |
//! |---------------------|--------------------|
//! | `Session`           | `session`          |
//! | `Invocation`        | `invocation`       |
//! | `LlmCall`           | `llm_call`         |
//! | `ToolExecution`     | `tool_execution`   |
//! | `ContextAssembly`   | `context_assembly` |
//! | `GuardianCheck`     | `guardian_check`   |
//! | `Compaction`        | `compaction`       |
//!
//! Kept separate from [`super::context`] (the
//! `SpanContext` data carrier) and [`super::builder`]
//! (the `SpanBuilder` orchestrator) so adding a new
//! span kind only touches this one file's enum +
//! name match.

/// Kind of span in the agent telemetry hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    /// Root span covering the entire session lifetime.
    Session,
    /// Span covering a single user input / ReAct loop iteration.
    Invocation,
    /// Span for an LLM API call.
    LlmCall,
    /// Span for tool execution.
    ToolExecution,
    /// Span for context assembly.
    ContextAssembly,
    /// Span for guardian safety checks.
    GuardianCheck,
    /// Span for context compaction.
    Compaction,
}

impl SpanKind {
    /// Returns the span name for this kind.
    pub fn name(&self) -> &'static str {
        match self {
            SpanKind::Session => "session",
            SpanKind::Invocation => "invocation",
            SpanKind::LlmCall => "llm_call",
            SpanKind::ToolExecution => "tool_execution",
            SpanKind::ContextAssembly => "context_assembly",
            SpanKind::GuardianCheck => "guardian_check",
            SpanKind::Compaction => "compaction",
        }
    }
}
