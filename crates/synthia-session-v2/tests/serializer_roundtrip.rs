use chrono::Utc;
use synthia_protocol::{CallId, MessageId, SessionId};
use synthia_session_v2::*;

#[test]
fn message_with_text_part_roundtrip() {
    let msg = Message {
        info: MessageInfo {
            id: MessageId::new(),
            parent_message_id: None,
            role: Role::User,
            time: MessageTime {
                created: Utc::now(),
                completed: None,
            },
            agent_name: None,
            model_id: None,
            trace: None,
            summary: false,
            error: None,
        },
        parts: vec![Part::Text(TextPart {
            text: "hello".to_string(),
            synthetic: false,
        })],
    };
    let json = serde_json::to_string(&msg).unwrap();
    let parsed: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.parts.len(), 1);
}

#[test]
fn tool_part_state_transitions() {
    let mut tp = ToolPart {
        call_id: CallId::new(),
        tool_name: "bash".to_string(),
        args: serde_json::json!({"cmd": "ls"}),
        state: ToolState::Pending {
            queued_at: Utc::now(),
        },
        metadata: Default::default(),
        attachments: vec![],
        time: ToolTime::default(),
    };
    tp.state = ToolState::Running {
        started_at: Utc::now(),
        partial_output: Some("file1\n".to_string()),
    };
    assert!(!tp.is_terminal());
    tp.state = ToolState::Completed {
        output: serde_json::json!("file1\nfile2"),
        ended_at: Utc::now(),
        duration_ms: 100,
    };
    assert!(tp.is_terminal());
}

#[test]
fn all_session_entry_variants_serialize() {
    let entries = vec![
        SessionEntry::Header {
            id: SessionId::new(),
            parent_id: None,
            created_at: Utc::now(),
            cli_version: "0.2.0".to_string(),
            rust_version: "1.85".to_string(),
            model_provider: "anthropic".to_string(),
            agent_name: "build".to_string(),
            agent_role: "coder".to_string(),
            sandbox_policy: "default".to_string(),
            approval_policy: "unless_trusted".to_string(),
            version: 2,
        },
        SessionEntry::Fork {
            id: MessageId::new(),
            parent_session_id: SessionId::new(),
            forked_at_message_id: MessageId::new(),
        },
        SessionEntry::Rollback {
            id: MessageId::new(),
            target_message_id: MessageId::new(),
            num_turns: 3,
        },
        SessionEntry::ErrorEvent {
            id: MessageId::new(),
            parent_message_id: None,
            error_kind: "tool_failure".to_string(),
            recoverable: false,
            payload: serde_json::json!({}),
        },
    ];
    for e in entries {
        let json = serde_json::to_string(&e).unwrap();
        let _parsed: SessionEntry = serde_json::from_str(&json).unwrap();
    }
}
