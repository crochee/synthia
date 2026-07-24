#![cfg(feature = "otel")]
//! Integration tests for the `compaction` span emitted by
//! `synthia_context::compaction_service::compact_messages` when the
//! `otel` cargo feature is enabled.
//!
//! Verifies the span records the OTel compaction semantic-convention
//! attributes required by the `agent-runtime-spans` spec:
//!  - `compaction.before_tokens` / `compaction.messages_before` /
//!    `compaction.stage` at span creation
//!  - `compaction.after_tokens` / `compaction.messages_after` recorded
//!    after compaction completes (declared as `tracing::field::Empty`
//!    at the callsite per the Task 7 lesson)
//!
//! The capture layer records both the initial fields (via
//! `on_new_span`) and the fields populated later via `Span::record`
//! (via `on_record`). Field values are stringified through the
//! `tracing::field::Visit::record_debug` trait — `usize` values are
//! captured with Debug formatting, while `%`-recorded Display values
//! (like `compaction.stage`) are captured without surrounding quotes.
//! All assertions use `contains(...)` for robustness.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use synthia_context::{TokenBudget, compaction_service::compact_messages};
use synthia_provider::Message;
use tracing_subscriber::{
    layer::{Context, Layer},
    prelude::*,
};

#[derive(Default, Debug)]
struct CapturedSpan {
    name: String,
    fields: HashMap<String, String>,
}

struct CaptureLayer {
    spans: Arc<Mutex<HashMap<tracing::span::Id, CapturedSpan>>>,
}

struct FieldVisitor<'a>(&'a mut HashMap<String, String>);

impl tracing::field::Visit for FieldVisitor<'_> {
    fn record_debug(
        &mut self,
        field: &tracing::field::Field,
        value: &dyn std::fmt::Debug,
    ) {
        self.0
            .insert(field.name().to_string(), format!("{value:?}"));
    }
}

impl<S: tracing::Subscriber> Layer<S> for CaptureLayer {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        _ctx: Context<'_, S>,
    ) {
        let mut span = CapturedSpan {
            name: attrs.metadata().name().to_string(),
            fields: HashMap::new(),
        };
        let mut visitor = FieldVisitor(&mut span.fields);
        attrs.record(&mut visitor);
        self.spans.lock().unwrap().insert(id.clone(), span);
    }

    fn on_record(
        &self,
        span: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        _ctx: Context<'_, S>,
    ) {
        let mut spans = self.spans.lock().unwrap();
        if let Some(span_data) = spans.get_mut(span) {
            let mut visitor = FieldVisitor(&mut span_data.fields);
            values.record(&mut visitor);
        }
    }
}

/// Install a `CaptureLayer` as the thread-local default subscriber and
/// return the shared `spans` map for assertions. The returned guard
/// keeps the subscriber alive until dropped (must outlive the
/// `compact_messages()` call under test).
fn install_capture_layer() -> (
    Arc<Mutex<HashMap<tracing::span::Id, CapturedSpan>>>,
    tracing::subscriber::DefaultGuard,
) {
    let spans: Arc<Mutex<HashMap<tracing::span::Id, CapturedSpan>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let guard = tracing_subscriber::registry::Registry::default()
        .with(CaptureLayer {
            spans: spans.clone(),
        })
        .set_default();
    (spans, guard)
}

/// Extract the single captured span named `compaction` from the map.
/// Panics if zero or more than one `compaction` span was captured.
fn expect_compaction_span(
    spans: &HashMap<tracing::span::Id, CapturedSpan>,
) -> &CapturedSpan {
    let compaction_spans: Vec<_> =
        spans.values().filter(|s| s.name == "compaction").collect();
    assert_eq!(
        compaction_spans.len(),
        1,
        "expected exactly one compaction span, got {}: {:?}",
        compaction_spans.len(),
        compaction_spans.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    compaction_spans[0]
}

fn assert_field_contains(
    fields: &HashMap<String, String>,
    name: &str,
    needle: &str,
) {
    match fields.get(name) {
        Some(v) => assert!(
            v.contains(needle),
            "field `{name}` should contain {needle:?}, got {v:?}"
        ),
        None => panic!(
            "field `{name}` should be present, but was not recorded. \
             Captured fields: {fields:?}"
        ),
    }
}

fn assert_field_present(fields: &HashMap<String, String>, name: &str) {
    assert!(
        fields.get(name).is_some(),
        "field `{name}` should be present, but was not recorded. \
         Captured fields: {fields:?}"
    );
}

/// Build a conversation large enough to trigger compaction under a
/// tight budget. 10 user/assistant pairs (20 messages) with ~50
/// chars each ≈ 250+ tokens, well over a `soft_limit` of 10.
fn build_large_conversation() -> Vec<Message> {
    let mut messages = Vec::new();
    for i in 0..10 {
        messages.push(Message::user(format!(
            "user message {i} with some content to add tokens"
        )));
        messages.push(Message::assistant(format!(
            "assistant response {i} with some content to add tokens"
        )));
    }
    messages
}

// =====================================================================
// Success path: compaction proceeds and span records all 5 attributes
// =====================================================================

#[test]
fn test_compact_messages_span_records_before_after_tokens_and_stage() {
    let mut messages = build_large_conversation();
    let messages_before = messages.len();
    let budget = TokenBudget::new(10);
    let token_count = 500;

    let (spans, _guard) = install_capture_layer();
    let result = compact_messages(&mut messages, &budget, token_count, 0.3);

    assert!(
        result.is_some(),
        "compaction should proceed (token_count=500 > soft_limit=10)"
    );
    let compaction_result = result.unwrap();
    assert!(
        compaction_result.old_tokens > compaction_result.new_tokens,
        "compaction must reduce token count: before={}, after={}",
        compaction_result.old_tokens,
        compaction_result.new_tokens
    );
    assert!(
        messages.len() < messages_before,
        "compaction must reduce message count: before={}, after={}",
        messages_before,
        messages.len()
    );

    let spans = spans.lock().unwrap();
    let span = expect_compaction_span(&spans);
    let fields = &span.fields;

    // Attributes set at span creation
    assert_field_contains(
        fields,
        "compaction.before_tokens",
        &compaction_result.old_tokens.to_string(),
    );
    assert_field_contains(
        fields,
        "compaction.messages_before",
        &messages_before.to_string(),
    );
    // stage is one of L1 / L2 / L3 (auto_select_level returned 1/2/3
    // because token_count=500 >> soft_limit=10 → ratio=50x → L3).
    // The `%stage_name` (Display) field is captured by `record_str`
    // → stored as the raw string without Debug quoting.
    assert_field_present(fields, "compaction.stage");
    let stage = fields.get("compaction.stage").unwrap();
    assert!(
        stage == "L1" || stage == "L2" || stage == "L3",
        "stage should be L1/L2/L3, got {stage:?}"
    );

    // Attributes recorded after compaction completes
    assert_field_contains(
        fields,
        "compaction.after_tokens",
        &compaction_result.new_tokens.to_string(),
    );
    assert_field_contains(
        fields,
        "compaction.messages_after",
        &messages.len().to_string(),
    );
}

// =====================================================================
// No-compaction path: span is NOT created when compaction is skipped
// =====================================================================

#[test]
fn test_compact_messages_no_span_when_within_budget() {
    let mut messages =
        vec![Message::user("hello"), Message::assistant("world")];
    let budget = TokenBudget::new(100_000);
    let token_count = 50;

    let (spans, _guard) = install_capture_layer();
    let result = compact_messages(&mut messages, &budget, token_count, 0.3);

    assert!(
        result.is_none(),
        "compaction should be skipped (token_count=50 <= soft_limit=100_000)"
    );

    let spans = spans.lock().unwrap();
    let compaction_spans: Vec<_> =
        spans.values().filter(|s| s.name == "compaction").collect();
    assert_eq!(
        compaction_spans.len(),
        0,
        "no compaction span should be emitted when compaction is skipped, \
         got: {:?}",
        compaction_spans.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
}

// =====================================================================
// Stage mapping: very high ratio selects L3
// =====================================================================

#[test]
fn test_compact_messages_span_stage_l3_for_high_ratio() {
    // ratio = token_count / soft_limit = 500 / 10 = 50x → L3 (>3.0)
    let mut messages = build_large_conversation();
    let budget = TokenBudget::new(10);
    let token_count = 500;

    let (spans, _guard) = install_capture_layer();
    let result = compact_messages(&mut messages, &budget, token_count, 0.3);
    assert!(result.is_some());

    let spans = spans.lock().unwrap();
    let span = expect_compaction_span(&spans);
    let stage = span.fields.get("compaction.stage");
    assert!(
        stage.is_some(),
        "compaction.stage should be recorded, fields: {:?}",
        span.fields
    );
    assert_eq!(
        stage.unwrap(),
        "L3",
        "ratio=50x should select L3 (marker-only), got {stage:?}"
    );
}
