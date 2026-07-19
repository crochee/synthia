//! Acceptance tests for PR-1.1 (`InMemoryEventBus` scaffold) re-anchored
//! on the PR-1.2 wire format.
//!
//! These tests pre-date PR-1.2; the `from_source` factory was replaced by
//! `from_prefix_hex` when PR-1.2 finalised `EventMeta`. The PR-1.1
//! behaviour surface (default-sink + emit-roundtrip + serde `From` + zero-cap
//! coercion) is unchanged.

#![allow(clippy::missing_const_for_fn)]

use synthia_event_v2::{
    EmitOutcome,
    EventBus,
    EventBusError,
    EventSinkKind,
    event::{EventMeta, EventSource},
    sink::in_memory::InMemoryEventBus,
};

/// PR-1.1 acceptance: the default sink is `InMemory` and is constructed
/// without pulling in `rusqlite` or any other external dependency.
#[test]
fn pr_1_1_default_sink_is_in_memory() {
    let bus = InMemoryEventBus::new();
    assert_eq!(bus.kind(), EventSinkKind::InMemory);
    assert_eq!(EventSinkKind::default(), EventSinkKind::InMemory);
    assert_eq!(EventSinkKind::InMemory.as_str(), "in_memory");
}

/// PR-1.1 acceptance: the trait signature round-trips a serializable
/// payload.
#[tokio::test]
async fn pr_1_1_emit_returns_buffered() {
    let bus = InMemoryEventBus::new();
    let payload = serde_json::json!({ "k": "v" });
    let result = bus.emit(&payload).await.expect("emit must succeed");
    assert_eq!(result, EmitOutcome::Buffered);
    assert!(!bus.is_closed());
}

/// PR-1.1 acceptance: serialisation errors map onto
/// [`EventBusError::Serialize`].
#[test]
fn pr_1_1_serialize_error_mapping_compiles() {
    fn assert_from<T: From<serde_json::Error>>() {}
    assert_from::<EventBusError>();
}

/// PR-1.1 acceptance: `EventSource` is wired through to the PR-1.2
/// `EventMeta` (no more `from_source` placeholder).
#[test]
fn pr_1_1_event_meta_carries_source() {
    let meta = EventMeta::from_prefix_hex(
        EventSource::Agent,
        1,
        &"00".repeat(32),
        None,
    );
    assert_eq!(meta.source, EventSource::Agent);
    assert_eq!(EventSource::Agent.as_str(), "agent");
    let default_meta = EventMeta::default();
    assert_eq!(default_meta.source, EventSource::System);
}

/// PR-1.1 acceptance: with-capacity never accepts `0`.
#[test]
fn pr_1_1_capacity_coerces_zero() {
    let bus = InMemoryEventBus::with_capacity(0);
    assert_eq!(bus.kind(), EventSinkKind::InMemory);
}
