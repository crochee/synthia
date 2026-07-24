use chrono::Utc;

use crate::types::{ColdEntry, HotEntry, MemoryEvent};

#[test]
fn memory_event_session_end() {
    let event = MemoryEvent::session_end(
        "session-123".to_string(),
        "Completed task".to_string(),
        vec!["read_file".to_string(), "bash".to_string()],
        "success".to_string(),
    );

    match event {
        MemoryEvent::SessionEnd {
            session_id,
            summary,
            tools_used,
            outcome,
        } => {
            assert_eq!(session_id, "session-123");
            assert_eq!(summary, "Completed task");
            assert_eq!(tools_used.len(), 2);
            assert_eq!(outcome, "success");
        }
        _ => panic!("expected SessionEnd variant"),
    }
}

#[test]
fn memory_event_tool_executed() {
    let event = MemoryEvent::tool_executed(
        "session-456".to_string(),
        "read_file".to_string(),
        true,
    );

    match event {
        MemoryEvent::ToolExecuted {
            session_id,
            tool_name,
            success,
        } => {
            assert_eq!(session_id, "session-456");
            assert_eq!(tool_name, "read_file");
            assert!(success);
        }
        _ => panic!("expected ToolExecuted variant"),
    }
}

#[test]
fn memory_event_serialize_deserialize() {
    let event = MemoryEvent::session_end(
        "session-789".to_string(),
        "Test summary".to_string(),
        vec!["tool1".to_string()],
        "success".to_string(),
    );

    let json = serde_json::to_string(&event).unwrap();
    let parsed: MemoryEvent = serde_json::from_str(&json).unwrap();

    match parsed {
        MemoryEvent::SessionEnd { session_id, .. } => {
            assert_eq!(session_id, "session-789");
        }
        _ => panic!("expected SessionEnd variant"),
    }
}

#[test]
fn hot_entry_default() {
    let entry = HotEntry::default();
    assert!(entry.key.is_empty());
    assert!(entry.value.is_empty());
    assert_eq!(entry.importance_score, 0.5);
}

#[test]
fn hot_entry_importance_score() {
    let entry = HotEntry {
        key: "test_key".to_string(),
        value: "test_value".to_string(),
        updated_at: Utc::now(),
        importance_score: 0.9,
    };
    assert_eq!(entry.importance_score, 0.9);
}

#[test]
fn cold_entry_default() {
    let entry = ColdEntry::default();
    assert!(entry.id.is_empty());
    assert!(entry.content.is_empty());
    assert_eq!(entry.metadata, serde_json::json!(null));
}

#[test]
fn cold_entry_with_metadata() {
    let metadata = serde_json::json!({"source": "test", "count": 42});
    let entry = ColdEntry {
        id: "entry-1".to_string(),
        content: "Test content".to_string(),
        metadata,
        created_at: Utc::now(),
        timestamp: None,
        summary: None,
        session_id: None,
        tools_used: None,
        outcome: None,
        importance_score: 0.75,
        access_count: 5,
    };

    assert_eq!(entry.id, "entry-1");
    assert_eq!(entry.content, "Test content");
    assert_eq!(entry.metadata["source"], "test");
    assert_eq!(entry.access_count, 5);
}
