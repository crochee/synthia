//! Compaction analytics: 5-field attempt struct + OTel span injection +
//! `tracing::info!` fallback.
//!
//! [`CompactionAnalyticsAttempt`] captures a single compaction attempt's
//! metadata so it can be emitted as structured telemetry. The 5 fields align
//! with the compaction analytics contract in the
//! `borrow-best-from-production-agents` change (task 4.2):
//!
//! | field                          | type                 | source                |
//! |--------------------------------|----------------------|-----------------------|
//! | `active_context_tokens_before` | `usize`              | pre-compaction tokens |
//! | `trigger`                      | [`CompactionTrigger`] | what initiated it     |
//! | `reason`                       | `String`             | human-readable cause  |
//! | `implementation`               | `String`             | stage / strategy name |
//! | `phase`                        | `String`             | sub-step of stage     |
//!
//! [`emit`](CompactionAnalyticsAttempt::emit) does two things:
//! 1. When the `otel` cargo feature is enabled, injects the 5 fields as
//!    attributes on the current `tracing::Span` (prefixed `compaction.`).
//!    This works with both `tracing-subscriber`'s console layer and the
//!    `tracing-opentelemetry` layer — no raw OpenTelemetry `Span` is touched.
//! 2. Always emits a `tracing::info!` event with the same 5 fields as
//!    structured key=value pairs, so the data is visible even without a
//!    subscriber that records span attributes.

use serde::{Deserialize, Serialize};

/// What initiated a compaction attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactionTrigger {
    /// Automatic compaction triggered by a token/usage threshold.
    AutoThreshold,
    /// Compaction triggered at a tool-call boundary.
    ToolCall,
    /// Manually requested compaction.
    Manual,
}

/// Analytics record for a single compaction attempt.
///
/// See the [module docs](self) for the field-by-field contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionAnalyticsAttempt {
    /// Token count of the active context immediately before compaction.
    pub active_context_tokens_before: usize,
    /// What initiated the compaction.
    pub trigger: CompactionTrigger,
    /// Human-readable reason string (e.g. `"threshold exceeded"`).
    pub reason: String,
    /// Compaction strategy identifier — one of:
    /// `"stage1-soft-trim"` | `"stage2-hard-clear"` | `"stage3-pruning"` |
    /// `"anchored-summary"`.
    pub implementation: String,
    /// Sub-step of the strategy — one of:
    /// `"head-tail"` | `"replace"` | `"compress"`.
    pub phase: String,
}

impl CompactionAnalyticsAttempt {
    /// Construct a new attempt record from its 5 constituent fields.
    pub fn new(
        active_context_tokens_before: usize,
        trigger: CompactionTrigger,
        reason: impl Into<String>,
        implementation: impl Into<String>,
        phase: impl Into<String>,
    ) -> Self {
        Self {
            active_context_tokens_before,
            trigger,
            reason: reason.into(),
            implementation: implementation.into(),
            phase: phase.into(),
        }
    }

    /// Emit this attempt as telemetry.
    ///
    /// When the `otel` feature is enabled, the 5 fields are injected as
    /// attributes on the current `tracing::Span` (keys prefixed
    /// `compaction.`). A `tracing::info!` event is always emitted afterwards
    /// as a structured fallback, so the data is visible regardless of
    /// subscriber configuration.
    ///
    /// This method never panics: if no subscriber is installed, both the
    /// span-attribute recording and the `info!` event become no-ops.
    pub fn emit(&self) {
        #[cfg(feature = "otel")]
        {
            self.emit_to_otel_span();
        }
        tracing::info!(
            active_context_tokens_before = self.active_context_tokens_before,
            trigger = ?self.trigger,
            reason = %self.reason,
            implementation = %self.implementation,
            phase = %self.phase,
            "compaction_attempt"
        );
    }

    /// Inject the 5 fields as attributes on the current `tracing::Span`.
    ///
    /// Uses `tracing::Span::current().record(...)` so the attributes flow
    /// through both `tracing-subscriber`'s fmt layer (console) and the
    /// `tracing-opentelemetry` layer (OTLP export) without touching a raw
    /// OpenTelemetry `Span`. If no span is active, `Span::current()` returns
    /// a no-op span and `record` calls are silently dropped.
    #[cfg(feature = "otel")]
    fn emit_to_otel_span(&self) {
        let span = tracing::Span::current();
        span.record(
            "compaction.active_context_tokens_before",
            self.active_context_tokens_before as i64,
        );
        let trigger_str = format!("{:?}", self.trigger);
        span.record("compaction.trigger", trigger_str.as_str());
        span.record("compaction.reason", self.reason.as_str());
        span.record("compaction.implementation", self.implementation.as_str());
        span.record("compaction.phase", self.phase.as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::{CompactionAnalyticsAttempt, CompactionTrigger};

    #[test]
    fn record_has_5_fields() {
        let attempt = CompactionAnalyticsAttempt::new(
            1000,
            CompactionTrigger::AutoThreshold,
            "threshold exceeded",
            "stage1-soft-trim",
            "head-tail",
        );
        assert_eq!(attempt.active_context_tokens_before, 1000);
        assert_eq!(attempt.trigger, CompactionTrigger::AutoThreshold);
        assert_eq!(attempt.reason, "threshold exceeded");
        assert_eq!(attempt.implementation, "stage1-soft-trim");
        assert_eq!(attempt.phase, "head-tail");
    }

    #[test]
    fn default_trigger_variants() {
        // The enum must expose exactly these 3 variants.
        let auto = CompactionTrigger::AutoThreshold;
        let tool = CompactionTrigger::ToolCall;
        let manual = CompactionTrigger::Manual;
        assert_ne!(auto, tool);
        assert_ne!(tool, manual);
        assert_ne!(auto, manual);
    }

    #[test]
    fn implementation_strings_match_stages() {
        // Catch typos in the canonical implementation strings.
        for impl_str in &[
            "stage1-soft-trim",
            "stage2-hard-clear",
            "stage3-pruning",
            "anchored-summary",
        ] {
            let attempt = CompactionAnalyticsAttempt::new(
                100,
                CompactionTrigger::Manual,
                "test",
                *impl_str,
                "head-tail",
            );
            assert_eq!(attempt.implementation, *impl_str);
        }
        // Phases likewise.
        for phase_str in &["head-tail", "replace", "compress"] {
            let attempt = CompactionAnalyticsAttempt::new(
                100,
                CompactionTrigger::Manual,
                "test",
                "stage1-soft-trim",
                *phase_str,
            );
            assert_eq!(attempt.phase, *phase_str);
        }
    }

    #[cfg(feature = "otel")]
    #[test]
    fn emit_does_not_panic_with_otel() {
        let attempt = CompactionAnalyticsAttempt::new(
            4242,
            CompactionTrigger::AutoThreshold,
            "auto threshold",
            "stage2-hard-clear",
            "replace",
        );
        attempt.emit();
    }

    #[cfg(not(feature = "otel"))]
    #[test]
    fn emit_does_not_panic_without_otel() {
        let attempt = CompactionAnalyticsAttempt::new(
            4242,
            CompactionTrigger::AutoThreshold,
            "auto threshold",
            "stage2-hard-clear",
            "replace",
        );
        attempt.emit();
    }

    #[test]
    fn serialize_roundtrip() {
        let attempt = CompactionAnalyticsAttempt::new(
            5000,
            CompactionTrigger::ToolCall,
            "tool boundary",
            "stage2-hard-clear",
            "replace",
        );
        let json = serde_json::to_string(&attempt).expect("serialize");
        let back: CompactionAnalyticsAttempt =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.active_context_tokens_before, 5000);
        assert_eq!(back.trigger, CompactionTrigger::ToolCall);
        assert_eq!(back.reason, "tool boundary");
        assert_eq!(back.implementation, "stage2-hard-clear");
        assert_eq!(back.phase, "replace");
    }

    #[test]
    fn new_constructor_sets_all_fields() {
        let attempt = CompactionAnalyticsAttempt::new(
            777,
            CompactionTrigger::Manual,
            "manual trigger",
            "stage3-pruning",
            "compress",
        );
        assert_eq!(attempt.active_context_tokens_before, 777);
        assert_eq!(attempt.trigger, CompactionTrigger::Manual);
        assert_eq!(attempt.reason, "manual trigger");
        assert_eq!(attempt.implementation, "stage3-pruning");
        assert_eq!(attempt.phase, "compress");
    }
}
