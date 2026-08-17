//! Unit tests for the `events` module family.
//!
//! Coverage map:
//!
//! - [`super::event_enum::AgentEvent`]: top-level 5-variant shape,
//!   serde round trip, helper ctors, `is_durable` correctness.

use synthia_provider::{
    ContentPart,
    ReasoningContent,
    SamplingResult,
    TokenUsage,
    ToolUse,
};

use super::*;

fn sampling_result_for_test() -> SamplingResult {
    SamplingResult {
        text: "hi".to_string(),
        tool_calls: vec![],
        reasoning: String::new(),
        reasoning_signature: None,
        usage: TokenUsage::default(),
        ..Default::default()
    }
}

#[test]
fn test_session_started_serialization() {
    let event = AgentEvent::System(SystemEvent::SessionStarted {
        session_id: "s1".to_string(),
    });
    let json = serde_json::to_string(&event).unwrap();
    // Both AgentEvent and SystemEvent use serde `tag = "type"`. The
    // outer "System" tag plus the inner "session_started" tag both
    // appear in the wire JSON.
    assert!(json.contains("\"type\":\"System\""));
    assert!(json.contains("\"type\":\"session_started\""));
    assert!(json.contains("s1"));
}

#[test]
fn test_event_serde_tag() {
    let event = AgentEvent::Model(ContentPart::Reasoning(ReasoningContent {
        text: "Let me think".to_string(),
        signature: None,
    }));
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"Model\""));
    assert!(json.contains("\"type\":\"reasoning\""));

    let event2 = AgentEvent::Model(ContentPart::ToolUse(ToolUse {
        id: "u1".to_string(),
        name: "read_file".to_string(),
        input: serde_json::json!({ "path": "/tmp/test" }),
    }));
    let json2 = serde_json::to_string(&event2).unwrap();
    assert!(json2.contains("\"type\":\"Model\""));

    let roundtripped: AgentEvent = serde_json::from_str(&json).unwrap();
    assert!(matches!(
        roundtripped,
        AgentEvent::Model(ContentPart::Reasoning(..))
    ));
}

#[test]
fn test_event_helper_methods() {
    let warning = AgentEvent::warning("low disk space");
    assert!(matches!(
        warning,
        AgentEvent::System(SystemEvent::Warning { .. })
    ));

    let progress = AgentEvent::progress("loading", 5, 10);
    assert!(matches!(
        progress,
        AgentEvent::System(SystemEvent::Progress {
            step: 5,
            total: 10,
            ..
        })
    ));

    let recovery =
        AgentEvent::recovery(3, Some("bash".into()), "fallback", None);
    assert!(matches!(
        recovery,
        AgentEvent::System(SystemEvent::Recovery {
            level_number: 3,
            ..
        })
    ));

    let usage = AgentEvent::usage(100, 50, Some(20), None);
    assert!(matches!(
        usage,
        AgentEvent::System(SystemEvent::Usage {
            input_tokens: 100,
            ..
        })
    ));
}

#[test]
fn test_is_durable_classification() {
    // Durable: text, tool use, tool result, resource.
    assert!(AgentEvent::text_delta("hi").is_durable());
    assert!(
        AgentEvent::Model(ContentPart::ToolUse(ToolUse {
            id: "u".into(),
            name: "n".into(),
            input: serde_json::json!({}),
        }))
        .is_durable()
    );
    assert!(
        AgentEvent::Model(ContentPart::Resource(
            synthia_provider::ResourceLink {
                uri: "file://x".into(),
                name: "x".into(),
                title: None,
                description: None,
                mime_type: None,
            }
        ))
        .is_durable()
    );

    // Ephemeral: reasoning, image, audio, ModelDone, System, Hook.
    assert!(!AgentEvent::reasoning_delta("thinking", None).is_durable());
    assert!(!AgentEvent::ModelDone(sampling_result_for_test()).is_durable());
    assert!(!AgentEvent::warning("disk").is_durable());
    assert!(!AgentEvent::progress("loading", 0, 1).is_durable());
    assert!(!AgentEvent::recovery(1, None, "msg", None).is_durable());
    assert!(!AgentEvent::usage(1, 1, None, None).is_durable());
}

#[test]
fn test_recursive_agent_event_durability() {
    // Inner is durable → wrapper is durable.
    let durable_inner = AgentEvent::text_delta("hi");
    let wrap = AgentEvent::Agent(
        AgentMeta::new("parent", "child", 1),
        Box::new(durable_inner),
    );
    assert!(wrap.is_durable());

    // Inner is ephemeral → wrapper is ephemeral.
    let ephemeral_inner = AgentEvent::warning("oops");
    let wrap = AgentEvent::Agent(
        AgentMeta::new("parent", "child", 1),
        Box::new(ephemeral_inner),
    );
    assert!(!wrap.is_durable());
}

#[test]
fn test_event_serde_round_trip() {
    use crate::events::SessionEndReason;

    let cases = [
        AgentEvent::text_delta("hi"),
        AgentEvent::System(SystemEvent::SessionStarted {
            session_id: "s".into(),
        }),
        AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed,
        }),
        AgentEvent::System(SystemEvent::SessionInterrupted {
            reason: "user".into(),
        }),
        AgentEvent::System(SystemEvent::Warning {
            kind: WarningKind::Loop,
            message: "loop".into(),
            iteration: Some(3),
        }),
        AgentEvent::System(SystemEvent::Recovery {
            level_number: 5,
            tool_name: Some("llm_sample".into()),
            message: "reset".into(),
            iteration: Some(7),
        }),
        AgentEvent::System(SystemEvent::Usage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: Some(20),
            cache_creation_tokens: None,
        }),
    ];
    for ev in cases {
        let json = serde_json::to_string(&ev).unwrap();
        let back: AgentEvent = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&back).unwrap();
        assert_eq!(json, json2, "round-trip failed for {ev:?}");
    }
}

#[test]
fn test_model_done_carries_sampling_result() {
    let event = AgentEvent::ModelDone(sampling_result_for_test());
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"ModelDone\""));
    assert!(json.contains("\"text\":\"hi\""));
}

#[test]
fn test_agent_meta_constructor() {
    let m = AgentMeta::new("p", "c", 2);
    assert_eq!(m.parent_session_id, "p");
    assert_eq!(m.child_session_id, "c");
    assert_eq!(m.parent_depth, 2);
}

#[test]
fn test_event_kind_label() {
    // Top-level kind label — collapses every ContentPart to
    // "Model" so log queries can filter on the outer variant.
    assert_eq!(AgentEvent::text_delta("hi").kind(), "Model");
    assert_eq!(
        AgentEvent::Model(ContentPart::ToolUse(ToolUse {
            id: "u".into(),
            name: "n".into(),
            input: serde_json::json!({}),
        }))
        .kind(),
        "Model"
    );
    assert_eq!(
        AgentEvent::System(SystemEvent::SessionStarted {
            session_id: "s".into()
        })
        .kind(),
        "System"
    );

    // SystemEvent inner kind label.
    let sys = SystemEvent::SessionStarted {
        session_id: "s".into(),
    };
    assert_eq!(sys.kind(), "SessionStarted");
    let sys = SystemEvent::SessionEnded {
        reason: SessionEndReason::Completed,
    };
    assert_eq!(sys.kind(), "SessionEnded");
    let sys = SystemEvent::Progress {
        message: "m".into(),
        step: 1,
        total: 2,
    };
    assert_eq!(sys.kind(), "Progress");
}

#[test]
fn test_tool_progress_system_event_uses_tool_output() {
    let ev = AgentEvent::System(SystemEvent::ToolProgress {
        tool_name: "bash".into(),
        call_id: "c1".into(),
        output: synthia_tool::ToolOutput::text("hello"),
    });
    let json = serde_json::to_string(&ev).unwrap();
    assert!(json.contains("\"type\":\"tool_progress\""));
    assert!(json.contains("bash"));
}

// =============================================================================
// AgentOutput — top-level return type from agent invocation.
// =============================================================================

/// `AgentOutput` MUST derive Clone + Debug (used to surface the
/// final session result alongside the streaming event log).
#[test]
fn agent_output_supports_clone_and_debug() {
    let o = AgentOutput {
        events: vec![AgentEvent::text_delta("hi")],
        final_message: Some("done".to_string()),
    };
    let _ = format!("{o:?}");
    let cloned = o.clone();
    assert_eq!(cloned.events.len(), 1);
    assert_eq!(cloned.final_message.as_deref(), Some("done"));
}

/// `AgentOutput` MUST support empty `events` + populated
/// `final_message` (the streaming-completed state when no
/// events survived buffering).
#[test]
fn agent_output_empty_events_with_final_message() {
    let o = AgentOutput {
        events: vec![],
        final_message: Some("final".to_string()),
    };
    assert!(o.events.is_empty());
    assert_eq!(o.final_message.as_deref(), Some("final"));
}

/// `AgentOutput` MUST support populated `events` + empty
/// `final_message` (the streaming state before aggregation).
#[test]
fn agent_output_events_with_no_final_message() {
    let o = AgentOutput {
        events: vec![
            AgentEvent::text_delta("hello "),
            AgentEvent::text_delta("world"),
        ],
        final_message: None,
    };
    assert_eq!(o.events.len(), 2);
    assert!(o.final_message.is_none());
}

/// `AgentOutput` MUST preserve event ordering in its `events`
/// field (events are appended in the order they are produced).
#[test]
fn agent_output_preserves_event_ordering() {
    let e1 = AgentEvent::text_delta("first");
    let e2 = AgentEvent::text_delta("second");
    let e3 = AgentEvent::progress("loading", 1, 3);
    let o = AgentOutput {
        events: vec![e1.clone(), e2.clone(), e3.clone()],
        final_message: None,
    };
    // Identity comparison via kind label (events don't impl
    // PartialEq).
    assert_eq!(o.events[0].kind(), e1.kind());
    assert_eq!(o.events[1].kind(), e2.kind());
    assert_eq!(o.events[2].kind(), e3.kind());
}

// =============================================================================
// SystemEvent 8-way enum serde + durability matrix
// =============================================================================

/// `SystemEvent` MUST round-trip every variant through JSON.
/// This is the contract that backends (sqlite, file, kafka)
/// rely on for replay.
#[test]
fn system_event_eight_variant_round_trip_through_json() {
    use crate::events::SessionEndReason;
    let cases = [
        SystemEvent::SessionStarted {
            session_id: "s".into(),
        },
        SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed,
        },
        SystemEvent::SessionInterrupted {
            reason: "user-cancel".into(),
        },
        SystemEvent::Progress {
            message: "m".into(),
            step: 1,
            total: 5,
        },
        SystemEvent::ToolProgress {
            tool_name: "bash".into(),
            call_id: "c1".into(),
            output: synthia_tool::ToolOutput::text("hello"),
        },
        SystemEvent::Warning {
            kind: WarningKind::Loop,
            message: "loop detected".into(),
            iteration: Some(3),
        },
        SystemEvent::Recovery {
            level_number: 3,
            tool_name: Some("llm_sample".into()),
            message: "fallback".into(),
            iteration: Some(5),
        },
        SystemEvent::Usage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: Some(20),
            cache_creation_tokens: None,
        },
    ];
    assert_eq!(cases.len(), 8);
    for ev in cases {
        let json = serde_json::to_string(&ev).unwrap();
        let parsed: SystemEvent =
            serde_json::from_str(&json).expect("round-trip");
        assert_eq!(parsed, ev, "round-trip lost {ev:?}");
    }
}

/// `SystemEvent` MUST use snake_case for the `type` tag
/// (the outer-tagged form of an `#[serde(tag = "type",
/// rename_all = "snake_case")]` enum). NOTE: `kind()` returns
/// PascalCase labels for log queries; the wire form is
/// snake_case. Pin both forms explicitly so a refactor that
/// changes either is caught loudly.
#[test]
fn system_event_all_variants_use_snake_case_type_tag() {
    use crate::events::SessionEndReason;
    let pairs: &[(SystemEvent, &str)] = &[
        (
            SystemEvent::SessionStarted {
                session_id: "s".into(),
            },
            "session_started",
        ),
        (
            SystemEvent::SessionEnded {
                reason: SessionEndReason::Completed,
            },
            "session_ended",
        ),
        (
            SystemEvent::SessionInterrupted { reason: "x".into() },
            "session_interrupted",
        ),
        (
            SystemEvent::Progress {
                message: "m".into(),
                step: 1,
                total: 2,
            },
            "progress",
        ),
        (
            SystemEvent::ToolProgress {
                tool_name: "bash".into(),
                call_id: "c1".into(),
                output: synthia_tool::ToolOutput::text("x"),
            },
            "tool_progress",
        ),
        (
            SystemEvent::Warning {
                kind: WarningKind::Hook,
                message: "x".into(),
                iteration: None,
            },
            "warning",
        ),
        (
            SystemEvent::Recovery {
                level_number: 1,
                tool_name: Some("llm_sample".into()),
                message: "x".into(),
                iteration: None,
            },
            "recovery",
        ),
        (
            SystemEvent::Usage {
                input_tokens: 1,
                output_tokens: 1,
                cache_read_tokens: None,
                cache_creation_tokens: None,
            },
            "usage",
        ),
    ];
    for (ev, expected_tag) in pairs {
        let json = serde_json::to_string(ev).unwrap();
        assert!(
            json.contains(&format!("\"type\":\"{expected_tag}\"")),
            "type tag for {ev:?} must be `{expected_tag}`: {json}"
        );
    }
}

/// `SystemEvent` MUST reject an unknown variant string at the
/// JSON layer (forward-compat: an external schema change must
/// not silently round-trip into the local enum).
#[test]
fn system_event_rejects_unknown_variant_string() {
    let json = r#"{"type":"future_variant","session_id":"x"}"#;
    let result: Result<SystemEvent, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

/// The 8 `SystemEvent` variants MUST all be distinct (no
/// accidental aliasing).
#[test]
fn system_event_eight_variants_are_distinct() {
    use crate::events::SessionEndReason;
    let all = [
        SystemEvent::SessionStarted {
            session_id: "s".into(),
        },
        SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed,
        },
        SystemEvent::SessionInterrupted { reason: "x".into() },
        SystemEvent::Progress {
            message: "m".into(),
            step: 1,
            total: 2,
        },
        SystemEvent::ToolProgress {
            tool_name: "bash".into(),
            call_id: "c1".into(),
            output: synthia_tool::ToolOutput::text("x"),
        },
        SystemEvent::Warning {
            kind: WarningKind::Loop,
            message: "x".into(),
            iteration: None,
        },
        SystemEvent::Recovery {
            level_number: 1,
            tool_name: Some("llm_sample".into()),
            message: "x".into(),
            iteration: None,
        },
        SystemEvent::Usage {
            input_tokens: 1,
            output_tokens: 1,
            cache_read_tokens: None,
            cache_creation_tokens: None,
        },
    ];
    assert_eq!(all.len(), 8);
    for i in 0..all.len() {
        for j in 0..all.len() {
            if i != j {
                assert_ne!(all[i], all[j], "{i} and {j} alias");
            }
        }
    }
}

/// `WarningKind` MUST support the 5 documented variants (no
/// aliasing, no missing variant).
#[test]
fn warning_kind_five_variants_are_distinct() {
    let all = [
        WarningKind::Loop,
        WarningKind::TokenBudget,
        WarningKind::ContextCompaction,
        WarningKind::Hook,
        WarningKind::EditConflict,
    ];
    assert_eq!(all.len(), 5);
    for i in 0..all.len() {
        for j in 0..all.len() {
            if i != j {
                assert_ne!(all[i], all[j], "{i} and {j} alias");
            }
        }
    }
}

// =============================================================================
// AgentEvent serde — JSON shape contracts for top-level enum.
// =============================================================================

/// `AgentEvent` MUST use `"type"` as the outer tag (the
/// internally-tagged form declared via
/// `#[serde(tag = "type", content = "data")]`).
#[test]
fn agent_event_outer_tag_is_type_field() {
    let ev = AgentEvent::text_delta("x");
    let json: serde_json::Value = serde_json::to_value(&ev).unwrap();
    assert!(json.get("type").is_some());
    assert!(json.get("data").is_some());
}

/// `AgentEvent::Agent(meta, inner)` MUST serialize both the
/// meta struct AND the inner event into the `data` slot.
#[test]
fn agent_event_agent_carries_meta_and_inner() {
    let inner = AgentEvent::text_delta("inner-text");
    let meta = AgentMeta::new("parent", "child", 1);
    let ev = AgentEvent::Agent(meta, Box::new(inner));
    let json: serde_json::Value = serde_json::to_value(&ev).unwrap();
    assert_eq!(json["type"], "Agent");
    // data is a tuple [meta, inner].
    let data = json["data"].as_array().expect("data must be array");
    assert_eq!(data.len(), 2);
    assert_eq!(data[0]["parent_session_id"], "parent");
    assert_eq!(data[0]["child_session_id"], "child");
    assert_eq!(data[0]["parent_depth"], 1);
    assert_eq!(data[1]["type"], "Model");
}

/// `AgentEvent` MUST reject an unknown outer tag.
#[test]
fn agent_event_rejects_unknown_outer_tag() {
    let json = r#"{"type":"UnknownVariant","data":{}}"#;
    let result: Result<AgentEvent, _> = serde_json::from_str(json);
    assert!(result.is_err());
}
