//! Verify type safety: ToolTime.compacted is Option<DateTime<Utc>>, not string or u64.

use chrono::{DateTime, Utc};
use synthia_session_v2::ToolTime;

#[test]
fn compacted_rejects_string() {
    let json = r#"{"compacted": "not-a-date"}"#;
    let result: Result<ToolTime, _> = serde_json::from_str(json);
    assert!(result.is_err(), "expected string to fail DateTime parse");
}

#[test]
fn compacted_rejects_u64() {
    let json = r#"{"compacted": 1234567890}"#;
    let result: Result<ToolTime, _> = serde_json::from_str(json);
    assert!(result.is_err(), "expected number to fail DateTime parse");
}

#[test]
fn compacted_accepts_iso8601() {
    let json = r#"{"compacted": "2026-07-12T10:30:00Z"}"#;
    let t: ToolTime = serde_json::from_str(json).unwrap();
    let dt: DateTime<Utc> = t.compacted.unwrap();
    assert_eq!(dt.to_rfc3339(), "2026-07-12T10:30:00+00:00");
}

#[test]
fn tool_state_pending_does_not_carry_output() {
    use std::collections::HashMap;

    use synthia_protocol::CallId;
    use synthia_session_v2::{ToolPart, ToolState, ToolTime};

    let part = ToolPart {
        call_id: CallId::new(),
        tool_name: "x".to_string(),
        args: serde_json::json!({}),
        state: ToolState::Pending {
            queued_at: Utc::now(),
        },
        metadata: HashMap::new(),
        attachments: vec![],
        time: ToolTime::default(),
    };
    match part.state {
        ToolState::Pending { .. } => {}
        _ => panic!("expected Pending"),
    }
}
