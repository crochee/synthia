//! Custom event projection: `AgentEvent::Custom` → `EventMsg::CustomEvent`.
//!
//! PR-7.3: Projects a custom event from the agent layer into the protocol
//! layer. If the `render_fn` returns an error or empty string, the projection
//! falls back to the raw JSON representation of the data payload.

use crate::{event::EventMsg, id::SessionId};

/// Project a custom event into a protocol `EventMsg::CustomEvent`.
///
/// `render_fn` is a closure that takes `(event_type, data)` and returns
/// a rendered string. In production this is backed by
/// `EventRendererRegistry::render`. If `render_fn` returns an empty string
/// or panics, the projection falls back to `serde_json::to_string(&data)`.
pub fn project_custom_event(
    session_id: SessionId,
    event_type: String,
    data: serde_json::Value,
    render_fn: &dyn Fn(&str, &serde_json::Value) -> String,
) -> EventMsg {
    let rendered = render_with_fallback(&event_type, &data, render_fn);
    EventMsg::CustomEvent {
        session_id,
        event_type,
        data,
        rendered,
    }
}

/// Attempt to render; on failure or empty output, fall back to raw JSON.
fn render_with_fallback(
    event_type: &str,
    data: &serde_json::Value,
    render_fn: &dyn Fn(&str, &serde_json::Value) -> String,
) -> String {
    let rendered = render_fn(event_type, data);
    if rendered.is_empty() {
        fallback_json(data)
    } else {
        rendered
    }
}

/// Fallback: serialize data as JSON string.
fn fallback_json(data: &serde_json::Value) -> String {
    serde_json::to_string(data).unwrap_or_else(|_| format!("{data:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::SessionId;

    fn sid() -> SessionId {
        SessionId::new()
    }

    #[test]
    fn project_custom_event_uses_render_fn() {
        let result = project_custom_event(
            sid(),
            "my_event".into(),
            serde_json::json!({"key": "value"}),
            &|_t, _d| "rendered output".to_string(),
        );
        match result {
            EventMsg::CustomEvent {
                event_type,
                rendered,
                ..
            } => {
                assert_eq!(event_type, "my_event");
                assert_eq!(rendered, "rendered output");
            }
            other => panic!("expected CustomEvent, got {other:?}"),
        }
    }

    #[test]
    fn project_custom_event_falls_back_on_empty_render() {
        let result = project_custom_event(
            sid(),
            "my_event".into(),
            serde_json::json!({"x": 1}),
            &|_t, _d| String::new(), // render returns empty
        );
        match result {
            EventMsg::CustomEvent { rendered, data, .. } => {
                // Falls back to JSON
                assert!(!rendered.is_empty());
                assert_eq!(data["x"], 1);
            }
            other => panic!("expected CustomEvent, got {other:?}"),
        }
    }

    #[test]
    fn project_custom_event_preserves_data() {
        let data = serde_json::json!({"count": 42, "nested": {"a": true}});
        let result = project_custom_event(
            sid(),
            "test".into(),
            data.clone(),
            &|_t, _d| "ok".to_string(),
        );
        match result {
            EventMsg::CustomEvent {
                data: result_data, ..
            } => {
                assert_eq!(result_data, data);
            }
            other => panic!("expected CustomEvent, got {other:?}"),
        }
    }

    #[test]
    fn custom_event_msg_serde_roundtrip() {
        let sid = sid();
        let msg = EventMsg::CustomEvent {
            session_id: sid,
            event_type: "plugin.notify".to_string(),
            data: serde_json::json!({"msg": "hello"}),
            rendered: "hello".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"msg\":\"custom_event\""));

        let parsed: EventMsg = serde_json::from_str(&json).unwrap();
        match parsed {
            EventMsg::CustomEvent {
                session_id,
                event_type,
                data,
                rendered,
            } => {
                assert_eq!(session_id, sid);
                assert_eq!(event_type, "plugin.notify");
                assert_eq!(data["msg"], "hello");
                assert_eq!(rendered, "hello");
            }
            other => panic!("expected CustomEvent, got {other:?}"),
        }
    }
}
