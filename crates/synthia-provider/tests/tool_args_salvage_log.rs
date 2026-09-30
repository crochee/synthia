//! Integration test for the `WARN` salvage line emitted by
//! [`synthia_provider::parse_tool_input_logged`].
//!
//! The provider stream processors and `BlockAssembler` finalize every
//! tool call through that helper: arguments the model malformed are
//! repaired (or passed through raw), and an operator reading
//! production logs must be able to see *that it happened* and *what
//! the payload was*. Valid arguments stay silent.

use std::sync::{Arc, Mutex};

use synthia_provider::parse_tool_input_logged;
use tracing::field::{Field, Visit};
use tracing_subscriber::{
    layer::{Context, Layer},
    prelude::*,
};

#[derive(Debug)]
struct CapturedEvent {
    level: tracing::Level,
    target: String,
    fields: Vec<(String, String)>,
}

#[derive(Default)]
struct FieldVisitor {
    fields: Vec<(String, String)>,
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields
            .push((field.name().to_string(), format!("{value:?}")));
    }
}

struct CaptureLayer {
    events: Arc<Mutex<Vec<CapturedEvent>>>,
}

impl<S: tracing::Subscriber> Layer<S> for CaptureLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        self.events
            .lock()
            .expect("events lock")
            .push(CapturedEvent {
                level: *event.metadata().level(),
                target: event.metadata().target().to_string(),
                fields: visitor.fields,
            });
    }
}

/// Install a capture layer as the thread-local default subscriber and
/// return the shared event vec plus the guard that keeps it alive.
fn capture() -> (
    Arc<Mutex<Vec<CapturedEvent>>>,
    tracing::subscriber::DefaultGuard,
) {
    let events: Arc<Mutex<Vec<CapturedEvent>>> =
        Arc::new(Mutex::new(Vec::new()));
    let guard = tracing_subscriber::registry::Registry::default()
        .with(CaptureLayer {
            events: events.clone(),
        })
        .set_default();
    (events, guard)
}

/// Field lookup by name. Values captured through `record_debug` are
/// formatted with `Debug`, so strings keep their surrounding quotes.
fn field<'a>(event: &'a CapturedEvent, name: &str) -> Option<&'a str> {
    event
        .fields
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

/// A repaired payload is logged once, at `WARN`, with the tool name,
/// the quality, and the salvaged text.
#[test]
fn repaired_arguments_are_logged_at_warn() {
    let (events, _guard) = capture();
    let raw = "{\"cmd\": \"echo one\necho two\"}";

    let value = parse_tool_input_logged(raw, "bash");

    assert_eq!(value["cmd"], "echo one\necho two");
    let events = events.lock().expect("events lock");
    assert_eq!(events.len(), 1, "one event per salvage: {events:?}");
    let event = &events[0];
    assert_eq!(event.level, tracing::Level::WARN);
    assert_eq!(event.target, "synthia_provider::json_repair");
    assert!(
        field(event, "tool").is_some_and(|tool| tool.contains("bash")),
        "tool name must be attached: {:?}",
        event.fields
    );
    assert_eq!(field(event, "quality"), Some("\"repaired\""));
    assert!(
        field(event, "message").is_some_and(|m| m.contains("repair")),
        "message must say what happened: {:?}",
        event.fields
    );
    let logged = field(event, "raw").expect("raw payload field");
    assert!(
        logged.contains("echo one") && logged.contains("echo two"),
        "the salvaged payload must be visible, got {logged}"
    );
}

/// An unparseable payload is logged at `WARN` too — the raw text still
/// reaches the tool, so the log is the only trace of the fallback.
#[test]
fn raw_fallback_is_logged_at_warn() {
    let (events, _guard) = capture();

    let value = parse_tool_input_logged("not json at all", "bash");

    assert_eq!(value, serde_json::json!("not json at all"));
    let events = events.lock().expect("events lock");
    assert_eq!(events.len(), 1, "one event per fallback: {events:?}");
    let event = &events[0];
    assert_eq!(event.level, tracing::Level::WARN);
    assert_eq!(field(event, "quality"), Some("\"raw-fallback\""));
    assert!(
        field(event, "raw").is_some_and(|raw| raw.contains("not json at all")),
        "the raw payload must be visible: {:?}",
        event.fields
    );
}

/// Valid arguments — and the empty "no arguments" payload — are the
/// normal case and must not log anything.
#[test]
fn valid_arguments_are_not_logged() {
    let (events, _guard) = capture();

    assert_eq!(
        parse_tool_input_logged(r#"{"cmd": "ls"}"#, "bash")["cmd"],
        "ls"
    );
    assert!(parse_tool_input_logged("", "bash").is_object());

    assert!(
        events.lock().expect("events lock").is_empty(),
        "a strict parse must not warn"
    );
}
