//! Integration tests for `SpanAttributesProcessor`.
//!
//! Verifies the four behaviours required by Task 3.11 / 3.12 of the
//! `otel-feature-integration` change:
//!
//! 1. `on_end` is a no-op (no panic, no side effect).
//! 2. `on_start` sets all 6 attributes to empty string `""` when no task-local
//!    context is present (spec: "Missing context field uses empty string").
//! 3. `on_start` injects all 6 attributes with their values when the full
//!    context is present.
//! 4. `on_start` only adds attributes — it does not modify the span's name,
//!    status, or kind.
//!
//! Additional coverage:
//!
//! 5. Partial context (only `user.id` missing) — other 5 attributes set
//!    normally, `user.id` set to `""` (spec scenario).
//! 6. No `statsig` dependency in the crate's `Cargo.toml`.
//!
//! The `on_start` tests build a real `SdkTracerProvider` with the processor
//! attached, start a span (which triggers `on_start`), then inspect the span
//! via `Span::exported_data()` before it is ended.

#![cfg(feature = "otel")]

use std::borrow::Cow;

use opentelemetry::{
    InstrumentationScope,
    trace::{SpanContext, SpanId, SpanKind, Status, Tracer, TracerProvider},
};
use opentelemetry_sdk::trace::{
    SdkTracerProvider,
    SpanData,
    SpanEvents,
    SpanLinks,
    SpanProcessor,
};
use synthia_telemetry::{
    SpanAttributesProcessor,
    span::attributes_processor::{
        AGENT_ID,
        GEN_AI_REQUEST_MODEL,
        GEN_AI_SYSTEM,
        SESSION_ID,
        TURN_ID,
        USER_ID,
    },
};

/// Attribute key constants (mirrored from the processor for assertion clarity).
const SESSION_ID_KEY: &str = "session.id";
const TURN_ID_KEY: &str = "turn.id";
const AGENT_ID_KEY: &str = "agent.id";
const USER_ID_KEY: &str = "user.id";
const GEN_AI_SYSTEM_KEY: &str = "gen_ai.system";
const GEN_AI_REQUEST_MODEL_KEY: &str = "gen_ai.request.model";

/// Count how many attributes on `data` match `key`.
fn attr_count(data: &SpanData, key: &str) -> usize {
    data.attributes
        .iter()
        .filter(|kv| kv.key.as_str() == key)
        .count()
}

/// Fetch the first attribute value for `key` as an owned `String`.
///
/// `Value::as_str` returns `Cow<'_, str>` (it may allocate for non-string
/// variants), so we normalize to `String` for assertion ergonomics.
fn attr_value(data: &SpanData, key: &str) -> Option<String> {
    data.attributes
        .iter()
        .find(|kv| kv.key.as_str() == key)
        .map(|kv| kv.value.as_str().into_owned())
}

/// Build a `SpanData` with minimal fields for `on_end` testing.
fn sample_span_data(name: &'static str) -> SpanData {
    SpanData {
        span_context: SpanContext::empty_context(),
        parent_span_id: SpanId::INVALID,
        parent_span_is_remote: false,
        span_kind: SpanKind::Internal,
        name: Cow::Borrowed(name),
        start_time: std::time::SystemTime::now(),
        end_time: std::time::SystemTime::now(),
        attributes: Vec::new(),
        dropped_attributes_count: 0,
        events: SpanEvents::default(),
        links: SpanLinks::default(),
        status: Status::default(),
        instrumentation_scope: InstrumentationScope::default(),
    }
}

// ----------------------------------------------------------------------------
// Test 1: on_end is a no-op
// ----------------------------------------------------------------------------

#[test]
fn on_end_is_noop() {
    // `on_end` consumes the `SpanData`; the contract is that it returns without
    // panicking and performs no work. We verify no panic and that the
    // processor remains usable afterwards.
    let processor = SpanAttributesProcessor::new();
    let data = sample_span_data("noop-span");

    // Should not panic.
    processor.on_end(data);

    // Processor is still usable (stateless, so no observable change).
    assert!(processor.force_flush().is_ok());
    assert!(processor.shutdown().is_ok());
}

// ----------------------------------------------------------------------------
// Test 2: missing context → all 6 attributes set to empty string
// ----------------------------------------------------------------------------

#[test]
fn missing_context_uses_empty_string() {
    // Outside any `sync_scope`, none of the task-locals are set. Per the spec
    // scenario "Missing context field uses empty string", `on_start` must:
    //   - not panic
    //   - set all 6 attributes to empty string `""`
    //   - inject exactly 6 attributes (one per slot)
    let provider = SdkTracerProvider::builder()
        .with_span_processor(SpanAttributesProcessor::new())
        .build();
    let tracer = provider.tracer("test/missing-context");

    // No task-local scope here — context is entirely absent.
    let span = tracer.span_builder("no-context-span").start(&tracer);
    let data = span
        .exported_data()
        .expect("span should be recording and have exportable data");

    // Exactly 6 attributes — all 6 slots are filled (with empty strings).
    assert_eq!(
        data.attributes.len(),
        6,
        "expected 6 attributes (all empty), got {:?}",
        data.attributes
    );

    // Each attribute value must be the empty string.
    for key in [
        SESSION_ID_KEY,
        TURN_ID_KEY,
        AGENT_ID_KEY,
        USER_ID_KEY,
        GEN_AI_SYSTEM_KEY,
        GEN_AI_REQUEST_MODEL_KEY,
    ] {
        assert_eq!(
            attr_value(&data, key).as_deref(),
            Some(""),
            "attribute `{key}` should be empty string when context is missing"
        );
    }

    // Cleanup: shut down the provider so background tasks exit cleanly.
    let _ = provider.force_flush();
    let _ = provider.shutdown();
}

// ----------------------------------------------------------------------------
// Test 3: injects all 6 attributes when context is present
// ----------------------------------------------------------------------------

#[test]
fn injects_all_six_attributes_when_context_present() {
    let provider = SdkTracerProvider::builder()
        .with_span_processor(SpanAttributesProcessor::new())
        .build();
    let tracer = provider.tracer("test/full-context");

    // Nest `sync_scope` for all 6 task-locals, then start a span inside the
    // innermost scope. `on_start` runs during `start`, so the task-locals are
    // visible.
    let data = SESSION_ID.sync_scope("sess-abc-123".to_string(), || {
        USER_ID.sync_scope("user-42".to_string(), || {
            AGENT_ID.sync_scope("agent-7".to_string(), || {
                TURN_ID.sync_scope("turn-001".to_string(), || {
                    GEN_AI_SYSTEM.sync_scope("anthropic".to_string(), || {
                        GEN_AI_REQUEST_MODEL.sync_scope(
                            "claude-3-5-sonnet-20241022".to_string(),
                            || {
                                let span = tracer
                                    .span_builder("full-context-span")
                                    .start(&tracer);
                                span.exported_data()
                                    .expect("span should be recording and have exportable data")
                            },
                        )
                    })
                })
            })
        })
    });

    // All 6 attributes should be present with the expected values.
    assert_eq!(attr_count(&data, SESSION_ID_KEY), 1);
    assert_eq!(
        attr_value(&data, SESSION_ID_KEY).as_deref(),
        Some("sess-abc-123")
    );

    assert_eq!(attr_count(&data, TURN_ID_KEY), 1);
    assert_eq!(attr_value(&data, TURN_ID_KEY).as_deref(), Some("turn-001"));

    assert_eq!(attr_count(&data, AGENT_ID_KEY), 1);
    assert_eq!(attr_value(&data, AGENT_ID_KEY).as_deref(), Some("agent-7"));

    assert_eq!(attr_count(&data, USER_ID_KEY), 1);
    assert_eq!(attr_value(&data, USER_ID_KEY).as_deref(), Some("user-42"));

    assert_eq!(attr_count(&data, GEN_AI_SYSTEM_KEY), 1);
    assert_eq!(
        attr_value(&data, GEN_AI_SYSTEM_KEY).as_deref(),
        Some("anthropic")
    );

    assert_eq!(attr_count(&data, GEN_AI_REQUEST_MODEL_KEY), 1);
    assert_eq!(
        attr_value(&data, GEN_AI_REQUEST_MODEL_KEY).as_deref(),
        Some("claude-3-5-sonnet-20241022")
    );

    // Exactly 6 attributes — no extras, no duplicates.
    assert_eq!(
        data.attributes.len(),
        6,
        "exactly 6 attributes expected, got {:?}",
        data.attributes
    );

    // Cleanup.
    let _ = provider.force_flush();
    let _ = provider.shutdown();
}

// ----------------------------------------------------------------------------
// Test 4: on_start does not modify span name / status / kind
// ----------------------------------------------------------------------------

#[test]
fn on_start_does_not_modify_span_name_or_status() {
    let provider = SdkTracerProvider::builder()
        .with_span_processor(SpanAttributesProcessor::new())
        .build();
    let tracer = provider.tracer("test/no-semantic-modification");

    // Use a distinctive name and Server kind so we can detect any mutation.
    const SPAN_NAME: &str = "preserve-semantics-span";

    let data = GEN_AI_SYSTEM.sync_scope("openai".to_string(), || {
        let span = tracer
            .span_builder(SPAN_NAME)
            .with_kind(SpanKind::Server)
            .start(&tracer);
        span.exported_data()
            .expect("span should be recording and have exportable data")
    });

    // Name must be unchanged.
    assert_eq!(data.name, SPAN_NAME);

    // Kind must be unchanged (Server, not mutated to Internal or anything else).
    assert_eq!(data.span_kind, SpanKind::Server);

    // Status must remain the default `Unset` — on_start must not set it.
    assert_eq!(data.status, Status::Unset);

    // Parent span id must be the invalid (root) — on_start must not re-parent.
    assert_eq!(data.parent_span_id, SpanId::INVALID);

    // When only `gen_ai.system` is set, the other 5 attributes must be empty
    // strings (not absent), per the missing-context contract.
    assert_eq!(
        data.attributes.len(),
        6,
        "exactly 6 attributes expected (1 set + 5 empty)"
    );
    assert_eq!(attr_count(&data, GEN_AI_SYSTEM_KEY), 1);
    assert_eq!(
        attr_value(&data, GEN_AI_SYSTEM_KEY).as_deref(),
        Some("openai")
    );
    // The other 5 must be empty strings.
    for key in [
        SESSION_ID_KEY,
        TURN_ID_KEY,
        AGENT_ID_KEY,
        USER_ID_KEY,
        GEN_AI_REQUEST_MODEL_KEY,
    ] {
        assert_eq!(
            attr_value(&data, key).as_deref(),
            Some(""),
            "attribute `{key}` should be empty string when only gen_ai.system is set"
        );
    }

    // Cleanup.
    let _ = provider.force_flush();
    let _ = provider.shutdown();
}

// ----------------------------------------------------------------------------
// Test 5: partial context — only `user.id` missing (spec scenario)
// ----------------------------------------------------------------------------

#[test]
fn partial_context_missing_user_id_uses_empty_string() {
    // Spec scenario "Missing context field uses empty string":
    //   WHEN on_start is invoked but user.id is not available (anonymous session)
    //   THEN the user.id attribute is set to empty string ""
    //   AND no panic occurs
    //   AND other attributes are still set normally
    let provider = SdkTracerProvider::builder()
        .with_span_processor(SpanAttributesProcessor::new())
        .build();
    let tracer = provider.tracer("test/partial-context");

    // Set 5 of 6 task-locals — `USER_ID` is intentionally absent.
    let data = SESSION_ID.sync_scope("sess-partial-1".to_string(), || {
        AGENT_ID.sync_scope("agent-9".to_string(), || {
            TURN_ID.sync_scope("turn-042".to_string(), || {
                GEN_AI_SYSTEM.sync_scope("openai".to_string(), || {
                    GEN_AI_REQUEST_MODEL.sync_scope("gpt-4o".to_string(), || {
                        let span = tracer
                            .span_builder("partial-context-span")
                            .start(&tracer);
                        span.exported_data()
                            .expect("span should be recording and have exportable data")
                    })
                })
            })
        })
    });

    // Exactly 6 attributes — all slots filled, `user.id` is the empty string.
    assert_eq!(data.attributes.len(), 6);

    // The 5 set attributes carry their values.
    assert_eq!(
        attr_value(&data, SESSION_ID_KEY).as_deref(),
        Some("sess-partial-1")
    );
    assert_eq!(attr_value(&data, AGENT_ID_KEY).as_deref(), Some("agent-9"));
    assert_eq!(attr_value(&data, TURN_ID_KEY).as_deref(), Some("turn-042"));
    assert_eq!(
        attr_value(&data, GEN_AI_SYSTEM_KEY).as_deref(),
        Some("openai")
    );
    assert_eq!(
        attr_value(&data, GEN_AI_REQUEST_MODEL_KEY).as_deref(),
        Some("gpt-4o")
    );

    // `user.id` must be the empty string (not absent).
    assert_eq!(attr_count(&data, USER_ID_KEY), 1);
    assert_eq!(
        attr_value(&data, USER_ID_KEY).as_deref(),
        Some(""),
        "user.id must be empty string when its task-local is not set"
    );

    // Cleanup.
    let _ = provider.force_flush();
    let _ = provider.shutdown();
}

// ----------------------------------------------------------------------------
// Test 6: no `statsig` dependency in Cargo.toml
// ----------------------------------------------------------------------------

#[test]
fn cargo_toml_has_no_statsig_dependency() {
    // Spec requirement "SpanAttributesProcessor SHALL NOT include Statsig
    // exporter": any reference to Statsig in the upstream codex implementation
    // MUST be stripped during porting. This test reads the crate's Cargo.toml
    // at compile time and asserts no `statsig` substring appears, providing a
    // compile-time regression guard.
    let cargo_toml = include_str!("../Cargo.toml");
    assert!(
        !cargo_toml.to_lowercase().contains("statsig"),
        "synthia-telemetry Cargo.toml must not reference statsig; found it in:\n{cargo_toml}"
    );
}
