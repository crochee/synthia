//! Integration tests for the `tool.execute` span emitted by
//! [`synthia_tool::ToolRegistry::run_stream`].
//!
//! Verifies the span records:
//!  - `tool.name` (from `Tool::name()`) at span creation
//!  - `exception.type = "TimeoutError"` + `otel.status_code = "ERROR"`
//!    when the tool returns an error output whose message indicates a
//!    timeout (the bash tool wraps `tokio::time::timeout` and surfaces
//!    "Command timed out ...")
//!  - `exception.type = "ToolError"` + `otel.status_code = "ERROR"` for
//!    other tool errors
//!
//! The capture layer records both the initial fields (via `on_new_span`)
//! and the fields populated later via `Span::record` (via `on_record`).
//!
//! Tests use a `current_thread` tokio runtime so that the thread-local
//! default subscriber (installed via `set_default`) is visible to the
//! `tokio::spawn`-ed tool tasks — they run on the same thread.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use synthia_tool::{Context, Tool, ToolEntry, ToolOutput, ToolRegistry};
use test_support::collect_results;
use tracing_subscriber::{
    layer::{Context as TracingContext, Layer},
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
        _ctx: TracingContext<'_, S>,
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
        _ctx: TracingContext<'_, S>,
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
/// `run_stream` drain under test).
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

/// Extract the single captured span named `tool.execute` from the map.
/// Panics if zero or more than one `tool.execute` span was captured.
fn expect_tool_execute_span(
    spans: &HashMap<tracing::span::Id, CapturedSpan>,
) -> &CapturedSpan {
    let tool_spans: Vec<_> = spans
        .values()
        .filter(|s| s.name == "tool.execute")
        .collect();
    assert_eq!(
        tool_spans.len(),
        1,
        "expected exactly one tool.execute span, got {}: {:?}",
        tool_spans.len(),
        tool_spans.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    tool_spans[0]
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

fn assert_field_absent(fields: &HashMap<String, String>, name: &str) {
    assert!(
        fields.get(name).is_none(),
        "field `{name}` should NOT be recorded, but got: {:?}. \
         Captured fields: {fields:?}",
        fields.get(name)
    );
}

fn make_context() -> Context {
    Context::new("s1".to_string(), PathBuf::from("/tmp"))
}

// =====================================================================
// Test tools
// =====================================================================

/// A tool that succeeds and returns a text output.
struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Echoes back a greeting"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        ToolOutput::text("ok")
    }
}

/// A tool that simulates a timeout by returning the same error
/// message shape the bash tool produces when its internal
/// `tokio::time::timeout` fires.
struct TimeoutTool;

#[async_trait]
impl Tool for TimeoutTool {
    fn name(&self) -> &str {
        "slow"
    }

    fn description(&self) -> &str {
        "A tool that times out"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        ToolOutput::error("Command timed out after 1 seconds")
    }
}

/// A tool that fails with a non-timeout error.
struct FailTool;

#[async_trait]
impl Tool for FailTool {
    fn name(&self) -> &str {
        "fail"
    }

    fn description(&self) -> &str {
        "A tool that always fails"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        ToolOutput::error("disk full")
    }
}

fn make_tool_use(name: &str) -> synthia_provider::ToolUse {
    synthia_provider::ToolUse {
        id: "1".to_string(),
        name: name.to_string(),
        input: serde_json::json!({}),
    }
}

// =====================================================================
// Tests
// =====================================================================

#[tokio::test(flavor = "current_thread")]
async fn test_tool_execute_span_has_tool_name() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(EchoTool)));

    let (spans, _guard) = install_capture_layer();
    let results = collect_results(
        registry.run_stream(vec![make_tool_use("echo")], make_context()),
        1,
    )
    .await;
    assert_eq!(results.len(), 1);
    assert!(results[0].1.is_text());

    let spans = spans.lock().unwrap();
    let span = expect_tool_execute_span(&spans);
    let fields = &span.fields;

    // tool.name recorded at span creation
    assert_field_contains(fields, "tool.name", "echo");
    // No exception attributes on success
    assert_field_absent(fields, "exception.type");
    assert_field_absent(fields, "exception.message");
    assert_field_absent(fields, "otel.status_code");
}

#[tokio::test(flavor = "current_thread")]
async fn test_tool_execute_span_timeout_records_timeout_error() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(TimeoutTool)));

    let (spans, _guard) = install_capture_layer();
    let results = collect_results(
        registry.run_stream(vec![make_tool_use("slow")], make_context()),
        1,
    )
    .await;
    assert_eq!(results.len(), 1);
    assert!(results[0].1.is_error.unwrap_or(false));

    let spans = spans.lock().unwrap();
    let span = expect_tool_execute_span(&spans);
    let fields = &span.fields;

    // tool.name still recorded even on failure
    assert_field_contains(fields, "tool.name", "slow");
    // Timeout mapped to TimeoutError exception type
    assert_field_contains(fields, "exception.type", "TimeoutError");
    assert_field_contains(fields, "exception.message", "timed out");
    assert_field_contains(fields, "otel.status_code", "ERROR");
}

#[tokio::test(flavor = "current_thread")]
async fn test_tool_execute_span_error_records_exception() {
    let registry = ToolRegistry::new();
    registry.register_entry(ToolEntry::new(Arc::new(FailTool)));

    let (spans, _guard) = install_capture_layer();
    let results = collect_results(
        registry.run_stream(vec![make_tool_use("fail")], make_context()),
        1,
    )
    .await;
    assert_eq!(results.len(), 1);
    assert!(results[0].1.is_error.unwrap_or(false));

    let spans = spans.lock().unwrap();
    let span = expect_tool_execute_span(&spans);
    let fields = &span.fields;

    assert_field_contains(fields, "tool.name", "fail");
    // Non-timeout error mapped to ToolError
    assert_field_contains(fields, "exception.type", "ToolError");
    assert_field_contains(fields, "exception.message", "disk full");
    assert_field_contains(fields, "otel.status_code", "ERROR");
}
