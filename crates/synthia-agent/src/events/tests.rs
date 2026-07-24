//! 10 unit tests for the `events` module family.
//!
//! Coverage map:
//!
//! - [`super::event_enum::AgentEvent`]: 6 tests
//!   (session_started_serialization / event_serde_tag /
//!   event_helper_methods /
//!   context_compacted_serialization /
//!   steering_received_serialization /
//!   session_ended_serialization /
//!   recovery_applied_event_roundtrip).
//! - [`super::emitter::AgentEventEmitter`]: 3 tests
//!   (pair / clone / returns_false_when_receiver_dropped).

use super::*;

#[test]
fn test_session_started_serialization() {
    let event = AgentEvent::SessionStarted {
        session_id: "s1".to_string(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("SessionStarted"));
    assert!(json.contains("s1"));
}

#[test]
fn test_event_serde_tag() {
    let event = AgentEvent::Thinking {
        text: "Let me think".to_string(),
        iteration: 1,
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"Thinking\""));

    let event2 = AgentEvent::ToolCallStarted {
        tool_name: "read_file".to_string(),
        input: serde_json::json!({ "path": "/tmp/test" }),
    };
    let json2 = serde_json::to_string(&event2).unwrap();
    assert!(json2.contains("\"type\":\"ToolCallStarted\""));

    let roundtripped: AgentEvent = serde_json::from_str(&json).unwrap();
    assert!(matches!(roundtripped, AgentEvent::Thinking { .. }));
}

#[test]
fn test_event_helper_methods() {
    let warning = AgentEvent::warning("low disk space");
    assert!(matches!(warning, AgentEvent::Warning { .. }));

    let progress = AgentEvent::progress("loading", 5, 10);
    assert!(matches!(
        progress,
        AgentEvent::Progress {
            step: 5,
            total: 10,
            ..
        }
    ));

    let thinking = AgentEvent::thinking("analyzing", 2);
    assert!(matches!(
        thinking,
        AgentEvent::Thinking { iteration: 2, .. }
    ));
}

#[test]
fn test_event_emitter_pair() {
    let (emitter, mut rx) = AgentEventEmitter::pair();
    emitter.emit(AgentEvent::SessionStarted {
        session_id: "test".to_string(),
    });
    let received = rx.try_recv().unwrap();
    assert!(matches!(received, AgentEvent::SessionStarted { .. }));
}

#[test]
fn test_event_emitter_clone() {
    let (emitter1, mut rx) = AgentEventEmitter::pair();
    let emitter2 = emitter1.clone();
    emitter1.emit(AgentEvent::IterationStarted { iteration: 1 });
    emitter2.emit(AgentEvent::IterationCompleted { iteration: 1 });
    assert!(rx.try_recv().is_ok());
    assert!(rx.try_recv().is_ok());
}

#[test]
fn test_emitter_returns_false_when_receiver_dropped() {
    let (emitter, rx) = AgentEventEmitter::pair();
    drop(rx);
    assert!(!emitter.emit(AgentEvent::Warning {
        message: "ignored".to_string()
    }));
}

#[test]
fn test_context_compacted_serialization() {
    let event = AgentEvent::ContextCompacted {
        old_tokens: 5000,
        new_tokens: 2500,
    };
    let json = serde_json::to_string(&event).unwrap();
    let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
    if let AgentEvent::ContextCompacted {
        old_tokens,
        new_tokens,
    } = parsed
    {
        assert_eq!(old_tokens, 5000);
        assert_eq!(new_tokens, 2500);
    } else {
        panic!("Expected ContextCompacted");
    }
}

#[test]
fn test_steering_received_serialization() {
    let event = AgentEvent::SteeringReceived {
        message: "focus on tests".to_string(),
        session_id: "s-1".to_string(),
        priority: Some(7),
    };
    let json = serde_json::to_string(&event).unwrap();
    let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
    if let AgentEvent::SteeringReceived {
        message,
        session_id,
        priority,
    } = parsed
    {
        assert_eq!(message, "focus on tests");
        assert_eq!(session_id, "s-1");
        assert_eq!(priority, Some(7));
    } else {
        panic!("Expected SteeringReceived");
    }

    // Backward compatibility: payloads without `priority` deserialize to None.
    let legacy_json = r#"{"type":"SteeringReceived","data":{"message":"legacy","session_id":"s-2"}}"#;
    let legacy: AgentEvent = serde_json::from_str(legacy_json).unwrap();
    if let AgentEvent::SteeringReceived { priority, .. } = legacy {
        assert_eq!(priority, None);
    } else {
        panic!("Expected SteeringReceived");
    }
}

#[test]
fn test_session_ended_serialization() {
    let event = AgentEvent::SessionEnded {
        reason: SessionEndReason::Completed,
    };
    let json = serde_json::to_string(&event).unwrap();
    let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
    assert!(matches!(
        parsed,
        AgentEvent::SessionEnded {
            reason: SessionEndReason::Completed
        }
    ));
}

#[test]
fn test_subagent_event_serialization() {
    let event = AgentEvent::SubagentEvent {
        child_session_id: "child-1".to_string(),
        event: Box::new(AgentEvent::Thinking {
            text: "nested thinking".to_string(),
            iteration: 2,
        }),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"subagent_event\""));
    assert!(json.contains("\"child_session_id\":\"child-1\""));
    assert!(json.contains("\"event\":{\"type\":\"Thinking\""));

    let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
    match parsed {
        AgentEvent::SubagentEvent {
            child_session_id,
            event,
        } => {
            assert_eq!(child_session_id, "child-1");
            assert!(matches!(event.as_ref(), AgentEvent::Thinking { .. }));
        }
        other => panic!("expected SubagentEvent, got {other:?}"),
    }
}

#[test]
fn test_subagent_completed_event_serializes() {
    let event = AgentEvent::SubagentCompleted {
        session_id: "s1".to_string(),
        result_summary: "done".to_string(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"SubagentCompleted\""));
    assert!(json.contains("\"session_id\":\"s1\""));
    assert!(json.contains("\"result_summary\":\"done\""));

    let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
    match parsed {
        AgentEvent::SubagentCompleted {
            session_id,
            result_summary,
        } => {
            assert_eq!(session_id, "s1");
            assert_eq!(result_summary, "done");
        }
        other => panic!("expected SubagentCompleted, got {other:?}"),
    }
}

#[test]
fn test_recovery_applied_event_roundtrip() {
    let event = AgentEvent::RecoveryApplied {
        level_number: 3,
        tool_name: Some("bash".to_string()),
        message: "Describing the command instead of executing".to_string(),
        iteration: 7,
    };
    let json = serde_json::to_string(&event).unwrap();
    let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
    match parsed {
        AgentEvent::RecoveryApplied {
            level_number,
            tool_name,
            message,
            iteration,
        } => {
            assert_eq!(level_number, 3);
            assert_eq!(tool_name.as_deref(), Some("bash"));
            assert_eq!(message, "Describing the command instead of executing");
            assert_eq!(iteration, 7);
        }
        other => panic!("expected RecoveryApplied, got {other:?}"),
    }
}

/// Helper: assert `is_durable_event_type` matches `is_durable` for a variant.
fn assert_classification_consistent(event: &AgentEvent) {
    let json = serde_json::to_value(event).unwrap();
    let type_tag = json
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("missing type tag in serialization: {json}"));
    assert_eq!(
        super::is_durable_event_type(type_tag),
        event.is_durable(),
        "classification mismatch for type tag {:?}",
        type_tag,
    );
}

#[test]
fn test_durable_event_classification_consistency() {
    use super::{AgentStatus, SessionEndReason};

    // Durable variants
    assert_classification_consistent(&AgentEvent::SessionStarted {
        session_id: "s".into(),
    });
    assert_classification_consistent(&AgentEvent::SessionEnded {
        reason: SessionEndReason::Completed,
    });
    assert_classification_consistent(&AgentEvent::LlmRequestStarted {
        iteration: 1,
    });
    assert_classification_consistent(&AgentEvent::LlmResponseComplete {
        content: "x".into(),
        usage: super::TokenUsage::default(),
    });
    assert_classification_consistent(&AgentEvent::ToolCallStarted {
        tool_name: "t".into(),
        input: serde_json::json!({}),
    });
    assert_classification_consistent(&AgentEvent::ToolCallCompleted {
        tool_name: "t".into(),
        output: "o".into(),
        is_error: false,
    });
    assert_classification_consistent(&AgentEvent::ToolCallSkipped {
        tool_name: "t".into(),
        reason: "r".into(),
    });
    assert_classification_consistent(&AgentEvent::ToolCallError {
        tool_name: "t".into(),
        error: "e".into(),
    });
    assert_classification_consistent(&AgentEvent::IterationStarted {
        iteration: 1,
    });
    assert_classification_consistent(&AgentEvent::ContextCompacted {
        old_tokens: 100,
        new_tokens: 50,
    });
    assert_classification_consistent(&AgentEvent::Checkpoint {
        session_id: "s".into(),
        step: 1,
    });
    assert_classification_consistent(&AgentEvent::StateChange {
        from: "a".into(),
        to: "b".into(),
    });
    assert_classification_consistent(&AgentEvent::RecoveryApplied {
        level_number: 1,
        tool_name: None,
        message: "m".into(),
        iteration: 1,
    });
    assert_classification_consistent(&AgentEvent::Status(AgentStatus::Running));
    assert_classification_consistent(&AgentEvent::SteeringReceived {
        message: "m".into(),
        session_id: "s".into(),
        priority: Some(3),
    });
    assert_classification_consistent(
        &AgentEvent::GuardianConfirmationRequest {
            tool_name: "t".into(),
            reason: "r".into(),
        },
    );
    assert_classification_consistent(&AgentEvent::SubagentSpawnBegin {
        session_id: "s".into(),
        agent_path: "p".into(),
    });
    assert_classification_consistent(&AgentEvent::SubagentSpawnEnd {
        session_id: "s".into(),
        agent_path: "p".into(),
        success: true,
        error: None,
    });
    assert_classification_consistent(&AgentEvent::SubagentComplete {
        session_id: "s".into(),
        agent_path: "p".into(),
        result: "r".into(),
    });
    assert_classification_consistent(&AgentEvent::Finish {
        output: "o".into(),
    });

    // Ephemeral variants
    assert_classification_consistent(&AgentEvent::LlmStreamDelta {
        content: "x".into(),
    });
    assert_classification_consistent(&AgentEvent::LlmReasoningDelta {
        delta: "x".into(),
    });
    assert_classification_consistent(&AgentEvent::LlmError {
        error: "e".into(),
    });
    assert_classification_consistent(&AgentEvent::IterationCompleted {
        iteration: 1,
    });
    assert_classification_consistent(&AgentEvent::Thinking {
        text: "t".into(),
        iteration: 1,
    });
    assert_classification_consistent(&AgentEvent::Warning {
        message: "m".into(),
    });
    assert_classification_consistent(&AgentEvent::Progress {
        message: "m".into(),
        step: 1,
        total: 2,
    });
    assert_classification_consistent(&AgentEvent::SessionInterrupted {
        reason: "r".into(),
    });
    assert_classification_consistent(&AgentEvent::GuardianWarning {
        reason: "r".into(),
        iteration: 1,
    });
    assert_classification_consistent(&AgentEvent::LoopWarning {
        reason: "r".into(),
        iteration: 1,
    });
    assert_classification_consistent(&AgentEvent::TokenBudgetNotice {
        status: "s".into(),
        current_tokens: 100,
        threshold_tokens: 200,
    });
    assert_classification_consistent(&AgentEvent::TokenBudgetWarning {
        status: "s".into(),
        current_tokens: 100,
        threshold_tokens: 200,
    });
    assert_classification_consistent(&AgentEvent::HookError {
        hook_name: "h".into(),
        error: "e".into(),
        hook_type: "t".into(),
    });
    assert_classification_consistent(&AgentEvent::SelfReflection {
        iteration: 1,
        summary: "s".into(),
        issues: vec![],
        suggestions: vec![],
    });
    assert_classification_consistent(&AgentEvent::SubagentMessage {
        session_id: "s".into(),
        agent_path: "p".into(),
        message: "m".into(),
    });
    assert_classification_consistent(&AgentEvent::SubagentCompleted {
        session_id: "s".into(),
        result_summary: "r".into(),
    });
    assert_classification_consistent(&AgentEvent::SubagentEvent {
        child_session_id: "c".into(),
        event: Box::new(AgentEvent::Thinking {
            text: "t".into(),
            iteration: 1,
        }),
    });
}

#[test]
fn test_is_durable_event_type_unknown_defaults_to_durable() {
    assert!(super::is_durable_event_type("UnknownType"));
    assert!(super::is_durable_event_type(""));
    assert!(super::is_durable_event_type("TurnStarted"));
    assert!(super::is_durable_event_type("SampleCompleted"));
    assert!(!super::is_durable_event_type("LlmStreamDelta"));
    assert!(!super::is_durable_event_type("Thinking"));
}
