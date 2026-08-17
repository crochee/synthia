//! `SpanAttributesProcessor` — auto-injects 6 standard span attributes on
//! span start.
//!
//! Implements `opentelemetry_sdk::trace::SpanProcessor`. In `on_start` it reads
//! [`tokio::task_local`] values populated by higher-level crates (e.g.
//! `synthia-agent` at the `Agent::run_stream` entry) and injects them as span
//! attributes using OpenTelemetry semantic conventions.
//!
//! # Why task-locals?
//!
//! `synthia-telemetry` cannot depend on `synthia-agent` / `synthia-core`'s
//! `SystemContext` without creating a circular dependency. Task-locals provide
//! a decoupled propagation channel: the agent crate sets the values, this
//! processor reads them. When a task-local is absent (e.g. anonymous session,
//! standalone test), the corresponding attribute is set to empty string `""` —
//! no panic, no `ERROR` log — per the spec scenario "Missing context field
//! uses empty string".
//!
//! # Injected attributes
//!
//! | Attribute              | Source task-local        | SemConv constant |
//! |------------------------|--------------------------|------------------|
//! | `session.id`           | [`SESSION_ID`]           | yes              |
//! | `user.id`              | [`USER_ID`]              | yes              |
//! | `agent.id`             | [`AGENT_ID`]             | no (custom)      |
//! | `turn.id`              | [`TURN_ID`]              | no (custom)      |
//! | `gen_ai.system`        | [`GEN_AI_SYSTEM`]        | yes              |
//! | `gen_ai.request.model` | [`GEN_AI_REQUEST_MODEL`] | yes              |
//!
//! `agent.id` and `turn.id` are synthia-specific attributes not present in
//! `opentelemetry-semantic-conventions` 0.27, so string literals are used.

use opentelemetry::{Context, KeyValue, trace::Span as SpanTrait};
use opentelemetry_sdk::trace::{Span, SpanData, SpanProcessor};
// OpenTelemetry semantic convention attribute keys. Aliased with an `_ATTR`
// suffix to avoid colliding with the task-local names below.
//
// `GEN_AI_*` are flagged `#[deprecated]` in the `opentelemetry-semantic-conventions`
// crate (the GenAI conventions moved to a separate repository), but the key
// strings (`"gen_ai.system"`, `"gen_ai.request.model"`) remain the agreed
// wire names for backend compatibility — keeping them rather than swapping
// to the new `gen_ai.provider.name` keys preserves telemetry consumers.
#[allow(deprecated)]
use opentelemetry_semantic_conventions::attribute::{
    GEN_AI_REQUEST_MODEL as GEN_AI_REQUEST_MODEL_ATTR,
    GEN_AI_SYSTEM as GEN_AI_SYSTEM_ATTR,
    SESSION_ID as SESSION_ID_ATTR,
    USER_ID as USER_ID_ATTR,
};

/// Custom attribute key for the agent instance ID.
///
/// Not part of the OpenTelemetry semantic conventions; synthia-specific.
const AGENT_ID_ATTR: &str = "agent.id";

/// Custom attribute key for the current turn ID.
///
/// Not part of the OpenTelemetry semantic conventions; synthia-specific.
const TURN_ID_ATTR: &str = "turn.id";

// Task-local context for span attribute injection.
//
// Higher-level crates (e.g. `synthia-agent`) populate these via
// `LocalKey::scope` / `LocalKey::sync_scope` around their execution. The
// processor reads them in `SpanAttributesProcessor::on_start`; if a task-local
// is absent the corresponding attribute is set to empty string `""` per the
// spec scenario "Missing context field uses empty string".
//
// All values are `String` (owned) so they can be cheaply cloned out via
// `LocalKey::try_get` without borrowing across the `on_start` boundary.
//
// (Regular comment, not a doc comment — rustdoc does not generate docs for
// `task_local!` macro invocations.)
tokio::task_local! {
    /// Current session ID (from `SystemContext`).
    pub static SESSION_ID: String;

    /// Current user ID (from `SystemContext` Source/Epoch, P1-4).
    pub static USER_ID: String;

    /// Agent instance ID.
    pub static AGENT_ID: String;

    /// Current turn ID.
    pub static TURN_ID: String;

    /// LLM provider name (e.g. `"anthropic"` / `"openai"`).
    pub static GEN_AI_SYSTEM: String;

    /// LLM request model name (e.g. `"claude-3-5-sonnet-20241022"`).
    pub static GEN_AI_REQUEST_MODEL: String;
}

/// A [`SpanProcessor`] that injects 6 standard span attributes on `on_start`.
///
/// The processor is stateless: it reads context exclusively from the
/// [`tokio::task_local`] values declared in this module. `on_end`,
/// `force_flush`, and `shutdown` are no-ops — the processor does not buffer or
/// export spans, it only enriches them in-place before they are handed to
/// downstream processors (e.g. the batch exporter).
///
/// # Example
///
/// ```ignore
/// use opentelemetry_sdk::trace::SdkTracerProvider;
/// use synthia_telemetry::SpanAttributesProcessor;
///
/// let provider = SdkTracerProvider::builder()
///     .with_span_processor(SpanAttributesProcessor::new())
///     .build();
/// ```
///
/// Assembly into the full tracing pipeline happens in `init_otlp_tracing`
/// (Task 4 of the `otel-feature-integration` change).
#[derive(Debug, Default)]
pub struct SpanAttributesProcessor;

impl SpanAttributesProcessor {
    /// Create a new `SpanAttributesProcessor`.
    ///
    /// The processor holds no state, so construction is trivial; `Default` is
    /// also implemented for ergonomics.
    pub const fn new() -> Self {
        Self
    }
}

impl SpanProcessor for SpanAttributesProcessor {
    #[allow(deprecated)]
    fn on_start(&self, span: &mut Span, _cx: &Context) {
        // Each task-local is tried independently via `try_get` (returns
        // `Result<String, AccessError>`). A missing task-local means the
        // corresponding context was not set (e.g. anonymous session, standalone
        // test); per the spec scenario "Missing context field uses empty
        // string", the attribute is set to empty string `""` — no panic, no
        // `ERROR` log. This ensures consumers can rely on the 6 attributes
        // always being present on spans emitted by this processor.
        let session_id = SESSION_ID.try_get().unwrap_or_default();
        SpanTrait::set_attribute(
            span,
            KeyValue::new(SESSION_ID_ATTR, session_id),
        );
        let turn_id = TURN_ID.try_get().unwrap_or_default();
        SpanTrait::set_attribute(span, KeyValue::new(TURN_ID_ATTR, turn_id));
        let agent_id = AGENT_ID.try_get().unwrap_or_default();
        SpanTrait::set_attribute(span, KeyValue::new(AGENT_ID_ATTR, agent_id));
        let user_id = USER_ID.try_get().unwrap_or_default();
        SpanTrait::set_attribute(span, KeyValue::new(USER_ID_ATTR, user_id));
        let gen_ai_system = GEN_AI_SYSTEM.try_get().unwrap_or_default();
        SpanTrait::set_attribute(
            span,
            KeyValue::new(GEN_AI_SYSTEM_ATTR, gen_ai_system),
        );
        let gen_ai_request_model =
            GEN_AI_REQUEST_MODEL.try_get().unwrap_or_default();
        SpanTrait::set_attribute(
            span,
            KeyValue::new(GEN_AI_REQUEST_MODEL_ATTR, gen_ai_request_model),
        );
    }

    fn on_end(&self, _span: SpanData) {
        // Intentional no-op: this processor only enriches spans on start.
        // Export/batch handling is delegated to downstream processors assembled
        // in `init_otlp_tracing`.
    }

    fn force_flush(&self) -> opentelemetry_sdk::error::OTelSdkResult {
        // Nothing to flush — the processor holds no buffered state.
        Ok(())
    }

    fn shutdown(&self) -> opentelemetry_sdk::error::OTelSdkResult {
        // Nothing to shut down — the processor holds no resources.
        Ok(())
    }

    fn shutdown_with_timeout(
        &self,
        _timeout: std::time::Duration,
    ) -> opentelemetry_sdk::error::OTelSdkResult {
        // Same as `shutdown` — no resources to release.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_returns_stateless_processor() {
        let _processor = SpanAttributesProcessor::new();
        // `Default` equivalent.
        let _default: SpanAttributesProcessor = Default::default();
    }

    #[test]
    fn force_flush_is_noop() {
        let processor = SpanAttributesProcessor::new();
        assert!(processor.force_flush().is_ok());
    }

    #[test]
    fn shutdown_is_noop() {
        let processor = SpanAttributesProcessor::new();
        assert!(processor.shutdown().is_ok());
        // Idempotent per SpanProcessor contract.
        assert!(processor.shutdown().is_ok());
    }
}
