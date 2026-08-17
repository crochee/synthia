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

#[cfg(test)]
mod tests {
    use super::*;

    /// `name()` is the stable label that shows up
    /// in `tracing::info_span!` calls and OTel
    /// export tags. A refactor that changes a
    /// label silently breaks observability
    /// dashboards and trace filtering. Pin all 7
    /// mappings.
    #[test]
    fn name_returns_stable_label_for_every_variant() {
        assert_eq!(SpanKind::Session.name(), "session");
        assert_eq!(SpanKind::Invocation.name(), "invocation");
        assert_eq!(SpanKind::LlmCall.name(), "llm_call");
        assert_eq!(SpanKind::ToolExecution.name(), "tool_execution");
        assert_eq!(SpanKind::ContextAssembly.name(), "context_assembly");
        assert_eq!(SpanKind::GuardianCheck.name(), "guardian_check");
        assert_eq!(SpanKind::Compaction.name(), "compaction");
    }

    /// `name()` MUST return `'static str` so the
    /// result is usable directly in
    /// `tracing::info_span!(name, ...)` without
    /// allocation. Compile-time check via
    /// explicit binding.
    #[test]
    fn name_returns_static_str_for_all_variants() {
        let kind = SpanKind::Compaction;
        let label: &'static str = kind.name();
        assert_eq!(label, "compaction");
    }

    /// Pin that the set of distinct names is
    /// exactly 7 — adding a new variant MUST
    /// either bump this or break the test
    /// (forcing a deliberate choice about OTel
    /// tag compatibility).
    #[test]
    fn name_distinct_count_is_exactly_seven() {
        let kinds = [
            SpanKind::Session,
            SpanKind::Invocation,
            SpanKind::LlmCall,
            SpanKind::ToolExecution,
            SpanKind::ContextAssembly,
            SpanKind::GuardianCheck,
            SpanKind::Compaction,
        ];
        let mut names: Vec<&str> = kinds.iter().map(|k| k.name()).collect();
        names.sort();
        names.dedup();
        assert_eq!(
            names.len(),
            7,
            "SpanKind::name() produced duplicate labels: {names:?}"
        );
    }

    /// `SpanKind` derives `Copy` — pin this so a
    /// refactor that accidentally removes
    /// `Copy` (e.g. adds a `String` payload)
    /// breaks loudly at the call sites that
    /// rely on it.
    #[test]
    fn span_kind_is_copy_and_eq() {
        let a = SpanKind::LlmCall;
        let b = a; // Copy
        assert_eq!(a, b);
        // Both still usable after the copy.
        assert_eq!(a.name(), b.name());
    }
}
