//! Unit tests for the `events` module family.
//!
//! Coverage map:
//!
//! - [`super::event_enum::AgentEvent`]: top-level 5-variant shape,
//!   serde round trip, helper ctors, `is_durable` correctness.
//! - [`super::emitter::AgentEventEmitter`]: pair / clone /
//!   returns_false_when_receiver_dropped.

use synthia_provider::{
    ContentPart,
    ReasoningContent,
    SamplingResult,
    TextContent,
    ToolResult,
    ToolUse,
};

use super::*;

fn sampling_result_for_test() -> SamplingResult {
    SamplingResult {
        text: "hi".to_string(),
        tool_calls: vec![],
        reasoning: String::new(),
        reasoning_signature: None,
        usage: super::TokenUsage::default(),
    }
}

#[test]
fn test_session_started_serialization() {
    let event = AgentEvent::System(SystemEvent::SessionStarted {
        session_id: "s1".to_string(),
    });
    let json = serde_json::to_string(&event).unwrap();
    // Both AgentEvent and SystemEvent use serde `tag = "type"`. Because
    // the inner SystemEvent's tag conflicts with the outer AgentEvent
    // tag at the same JSON level, the outer "system" tag is suppressed
    // by serde and only the inner SystemEvent variant tag is emitted.
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

    let delta = AgentEvent::text_delta("analyzing");
    assert!(matches!(
        delta,
        AgentEvent::Model(ContentPart::Text(TextContent { .. }))
    ));

    let reasoning = AgentEvent::reasoning_delta("thinking", None);
    assert!(matches!(
        reasoning,
        AgentEvent::Model(ContentPart::Reasoning(ReasoningContent {
            ref text,
            ..
        })) if text == "thinking"
    ));
}

#[test]
fn test_event_emitter_pair() {
    let (emitter, mut rx) = AgentEventEmitter::pair();
    emitter.emit(AgentEvent::System(SystemEvent::SessionStarted {
        session_id: "test".to_string(),
    }));
    let received = rx.try_recv().unwrap();
    assert!(matches!(
        received,
        AgentEvent::System(SystemEvent::SessionStarted { .. })
    ));
}

#[test]
fn test_event_emitter_clone() {
    let (emitter1, mut rx) = AgentEventEmitter::pair();
    let emitter2 = emitter1.clone();
    emitter1.emit(AgentEvent::System(SystemEvent::Progress {
        message: "a".into(),
        step: 1,
        total: 2,
    }));
    emitter2.emit(AgentEvent::System(SystemEvent::Progress {
        message: "b".into(),
        step: 2,
        total: 2,
    }));
    assert!(rx.try_recv().is_ok());
    assert!(rx.try_recv().is_ok());
}

#[test]
fn test_emitter_returns_false_when_receiver_dropped() {
    let (emitter, rx) = AgentEventEmitter::pair();
    drop(rx);
    assert!(!emitter.emit(AgentEvent::System(SystemEvent::Warning {
        kind: WarningKind::Hook,
        message: "ignored".to_string(),
        iteration: None,
    })));
}

#[test]
fn test_context_compaction_warning_serialization() {
    let event = AgentEvent::System(SystemEvent::Warning {
        kind: WarningKind::ContextCompaction,
        message: "compacted 5000 -> 2500".to_string(),
        iteration: Some(3),
    });
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"System\""));
    let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
    match parsed {
        AgentEvent::System(SystemEvent::Warning {
            kind,
            message,
            iteration,
        }) => {
            assert_eq!(kind, WarningKind::ContextCompaction);
            assert_eq!(message, "compacted 5000 -> 2500");
            assert_eq!(iteration, Some(3));
        }
        other => panic!("Expected System::Warning, got {other:?}"),
    }
}

#[test]
fn test_hook_message_serialization() {
    let event = AgentEvent::Hook(HookEvent::Message {
        priority: 7,
        message: "focus on tests".to_string(),
    });
    let json = serde_json::to_string(&event).unwrap();
    // Both AgentEvent and HookEvent use serde `tag = "type"`; the outer
    // "hook" tag is suppressed by the same key collision (the inner
    // HookEvent variant tag wins). The A2A adapter translates to the
    // documented `kind` discriminator format on the wire.
    assert!(json.contains("\"type\":\"message\""));
    let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
    match parsed {
        AgentEvent::Hook(HookEvent::Message { priority, message }) => {
            assert_eq!(priority, 7);
            assert_eq!(message, "focus on tests");
        }
        other => panic!("Expected Hook::Message, got {other:?}"),
    }
}

#[test]
fn test_session_ended_serialization() {
    let event = AgentEvent::System(SystemEvent::SessionEnded {
        reason: SessionEndReason::Completed,
    });
    let json = serde_json::to_string(&event).unwrap();
    let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
    assert!(matches!(
        parsed,
        AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed
        })
    ));
}

#[test]
fn test_subagent_event_serialization() {
    let inner = AgentEvent::Model(ContentPart::Reasoning(ReasoningContent {
        text: "nested thinking".to_string(),
        signature: None,
    }));
    let event = AgentEvent::Agent(
        AgentMeta::new("parent-1", "child-1", 1),
        Box::new(inner),
    );
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"Agent\""));
    assert!(json.contains("\"parent_session_id\":\"parent-1\""));
    assert!(json.contains("\"child_session_id\":\"child-1\""));

    let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
    match parsed {
        AgentEvent::Agent(meta, inner) => {
            assert_eq!(meta.parent_session_id, "parent-1");
            assert_eq!(meta.child_session_id, "child-1");
            assert_eq!(meta.parent_depth, 1);
            assert!(matches!(
                inner.as_ref(),
                AgentEvent::Model(ContentPart::Reasoning(..))
            ));
        }
        other => panic!("expected Agent, got {other:?}"),
    }
}

#[test]
fn test_recovery_event_roundtrip() {
    let event = AgentEvent::recovery(
        3,
        Some("bash".to_string()),
        "Describing the command instead of executing",
        Some(7),
    );
    let json = serde_json::to_string(&event).unwrap();
    let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
    match parsed {
        AgentEvent::System(SystemEvent::Recovery {
            level_number,
            tool_name,
            message,
            iteration,
        }) => {
            assert_eq!(level_number, 3);
            assert_eq!(tool_name.as_deref(), Some("bash"));
            assert_eq!(message, "Describing the command instead of executing");
            assert_eq!(iteration, Some(7));
        }
        other => panic!("expected System::Recovery, got {other:?}"),
    }
}

#[test]
fn test_is_durable_top_level() {
    // Model(Text) is durable — it's the text of the assistant message
    // that needs to be replayed to reconstruct LoopContext.
    let m = AgentEvent::Model(ContentPart::Text(TextContent {
        text: "x".into(),
        cache_control: None,
    }));
    assert!(m.is_durable());

    // ModelDone is ephemeral — it is the aggregated final result;
    // replay can recompute it from the persisted Model deltas.
    let md = AgentEvent::ModelDone(sampling_result_for_test());
    assert!(!md.is_durable());

    // System(_): per spec all System variants are ephemeral
    // (was: SessionStarted/Ended/Interrupted/Recovery were durable in
    // the old implementation; the new spec unifies System as
    // ephemeral because session lifecycle events are reconstructed
    // from replay, not persisted as state transitions).
    let started = AgentEvent::System(SystemEvent::SessionStarted {
        session_id: "s".into(),
    });
    assert!(!started.is_durable());

    let ended = AgentEvent::System(SystemEvent::SessionEnded {
        reason: SessionEndReason::Completed,
    });
    assert!(!ended.is_durable());

    let interrupted = AgentEvent::System(SystemEvent::SessionInterrupted {
        reason: "boom".into(),
    });
    assert!(!interrupted.is_durable());

    let progress = AgentEvent::progress("a", 1, 2);
    assert!(!progress.is_durable());

    let warning = AgentEvent::warning("warn");
    assert!(!warning.is_durable());

    let recovery = AgentEvent::recovery(1, None, "m", None);
    assert!(!recovery.is_durable());

    let usage = AgentEvent::usage(1, 2, None, None);
    assert!(!usage.is_durable());

    // Hook is ephemeral
    let hook = AgentEvent::Hook(HookEvent::Message {
        priority: 0,
        message: "x".into(),
    });
    assert!(!hook.is_durable());

    // Agent inherits durability from the inner event:
    //   inner = Model(Text)  -> durable (true)
    //   inner = ModelDone   -> ephemeral (false)
    let inner_durable = AgentEvent::Agent(
        AgentMeta::new("p", "c", 0),
        Box::new(AgentEvent::Model(ContentPart::Text(TextContent {
            text: "x".into(),
            cache_control: None,
        }))),
    );
    assert!(inner_durable.is_durable());

    let inner_ephemeral = AgentEvent::Agent(
        AgentMeta::new("p", "c", 0),
        Box::new(AgentEvent::ModelDone(sampling_result_for_test())),
    );
    assert!(!inner_ephemeral.is_durable());
}

#[test]
fn test_all_top_level_variants_constructible() {
    // 1. Model
    let _ = AgentEvent::Model(ContentPart::Text(TextContent {
        text: "x".into(),
        cache_control: None,
    }));
    // 2. ModelDone
    let _ = AgentEvent::ModelDone(sampling_result_for_test());
    // 3. System
    let _ = AgentEvent::System(SystemEvent::SessionStarted {
        session_id: "s".into(),
    });
    // 4. Agent
    let _ = AgentEvent::Agent(
        AgentMeta::new("p", "c", 0),
        Box::new(AgentEvent::ModelDone(sampling_result_for_test())),
    );
    // 5. Hook
    let _ = AgentEvent::Hook(HookEvent::Message {
        priority: 0,
        message: "x".into(),
    });

    // All SystemEvent variants:
    let _ = AgentEvent::System(SystemEvent::SessionEnded {
        reason: SessionEndReason::Completed,
    });
    let _ = AgentEvent::System(SystemEvent::SessionInterrupted {
        reason: "x".into(),
    });
    let _ = AgentEvent::System(SystemEvent::Progress {
        message: "x".into(),
        step: 1,
        total: 2,
    });
    let _ = AgentEvent::System(SystemEvent::Warning {
        kind: WarningKind::Guardian,
        message: "x".into(),
        iteration: Some(1),
    });
    let _ = AgentEvent::recovery(1, None, "x", Some(1));
    let _ = AgentEvent::usage(1, 2, Some(3), Some(4));

    // All WarningKind variants:
    for kind in [
        WarningKind::Guardian,
        WarningKind::Loop,
        WarningKind::TokenBudget,
        WarningKind::ContextCompaction,
        WarningKind::Hook,
        WarningKind::EditConflict,
    ] {
        let _ = AgentEvent::warning_kind(kind, "x");
    }

    // All HookEvent variants:
    let _ = AgentEvent::Hook(HookEvent::ConfirmRequest {
        tool_use_id: "u1".into(),
        tool_name: "bash".into(),
        reason: "needs approval".into(),
    });
    let _ = AgentEvent::Hook(HookEvent::ConfirmResponse {
        approved: true,
        tool_use_id: "u1".into(),
    });
    let _ = AgentEvent::Hook(HookEvent::Custom {
        kind: "my_ext.event".into(),
        data: serde_json::json!({}),
    });

    // Model content includes ToolUse + ToolResult
    let _ = AgentEvent::Model(ContentPart::ToolUse(ToolUse {
        id: "u1".into(),
        name: "bash".into(),
        input: serde_json::json!({}),
    }));
    let _ =
        AgentEvent::Model(ContentPart::ToolResult(ToolResult::new("u1", "ok")));
}

#[test]
fn test_hook_event_custom_serde_roundtrip() {
    let event = AgentEvent::Hook(HookEvent::Custom {
        kind: "my_plugin.event".to_string(),
        data: serde_json::json!({"key": "value", "count": 42}),
    });
    let json = serde_json::to_string(&event).unwrap();
    // Same serde tag collision as test_hook_message_serialization —
    // the inner HookEvent::Custom variant tag wins.
    assert!(json.contains("\"type\":\"custom\""));
    assert!(json.contains("\"key\":\"value\""));

    let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
    match parsed {
        AgentEvent::Hook(HookEvent::Custom { kind, data }) => {
            assert_eq!(kind, "my_plugin.event");
            assert_eq!(data["key"], "value");
            assert_eq!(data["count"], 42);
        }
        other => panic!("expected Hook::Custom, got {other:?}"),
    }
}

#[test]
fn test_tool_result_in_model_preserves_is_error() {
    let ok =
        AgentEvent::Model(ContentPart::ToolResult(ToolResult::new("u1", "ok")));
    let ok_json = serde_json::to_string(&ok).unwrap();
    let ok_parsed: AgentEvent = serde_json::from_str(&ok_json).unwrap();
    match ok_parsed {
        AgentEvent::Model(ContentPart::ToolResult(tr)) => {
            assert_eq!(tr.tool_use_id, "u1");
            // ToolResult::new sets is_error = None (not Some(false));
            // a successful tool result has no error tag.
            assert_eq!(tr.is_error, None);
        }
        other => panic!("expected Model::ToolResult, got {other:?}"),
    }

    let err = AgentEvent::Model(ContentPart::ToolResult(ToolResult::error(
        "u1", "boom",
    )));
    let err_json = serde_json::to_string(&err).unwrap();
    let err_parsed: AgentEvent = serde_json::from_str(&err_json).unwrap();
    match err_parsed {
        AgentEvent::Model(ContentPart::ToolResult(tr)) => {
            assert_eq!(tr.is_error, Some(true));
        }
        other => panic!("expected Model::ToolResult, got {other:?}"),
    }
}

#[test]
fn test_is_durable_event_type_unknown_defaults_to_durable() {
    // Persistence-layer type-tag whitelist (the string-tagged analogue
    // of `AgentEvent::is_durable`). The exhaustive, agent-layer
    // method is tested in `test_agent_event_is_durable` below.
    assert!(super::is_durable_event_type("UnknownType"));
    assert!(super::is_durable_event_type(""));
    assert!(super::is_durable_event_type("TurnStarted"));
    assert!(super::is_durable_event_type("SampleCompleted"));
    assert!(super::is_durable_event_type("ToolCallStarted"));
    assert!(super::is_durable_event_type("ToolCallCompleted"));
    assert!(!super::is_durable_event_type("ModelText"));
    assert!(!super::is_durable_event_type("ModelReasoning"));
    assert!(!super::is_durable_event_type("ModelImage"));
    assert!(!super::is_durable_event_type("ModelAudio"));
    assert!(!super::is_durable_event_type("ModelResource"));
    assert!(!super::is_durable_event_type("Hook"));
    assert!(!super::is_durable_event_type("SteeringReceived"));
    assert!(!super::is_durable_event_type("Progress"));
    assert!(!super::is_durable_event_type("TokenBudgetNotice"));
}

#[test]
fn test_agent_event_is_durable_exhaustive() {
    // Durable paths (per event-durability-classification spec).
    assert!(
        AgentEvent::Model(ContentPart::Text(TextContent {
            text: "hi".into(),
            cache_control: None,
        }))
        .is_durable()
    );
    assert!(
        AgentEvent::Model(ContentPart::ToolUse(ToolUse {
            id: "tu_1".into(),
            name: "search".into(),
            input: serde_json::json!({}),
        }))
        .is_durable()
    );
    assert!(
        AgentEvent::Model(ContentPart::ToolResult(ToolResult {
            tool_use_id: "tu_1".into(),
            content: vec![],
            structured_content: None,
            is_error: None,
        }))
        .is_durable()
    );
    assert!(
        AgentEvent::Model(ContentPart::Resource(
            synthia_provider::ResourceLink {
                uri: String::new(),
                name: String::new(),
                title: None,
                description: None,
                mime_type: None,
            }
        ))
        .is_durable()
    );

    // Ephemeral paths.
    assert!(
        !AgentEvent::Model(ContentPart::Reasoning(ReasoningContent {
            text: "thinking".into(),
            signature: None,
        }))
        .is_durable()
    );
    assert!(
        !AgentEvent::Model(ContentPart::Image(
            synthia_provider::ImageContent {
                data: String::new(),
                mime_type: String::new(),
                detail: None,
            }
        ))
        .is_durable()
    );
    assert!(
        !AgentEvent::Model(ContentPart::Audio(
            synthia_provider::types::AudioContent {
                data: String::new(),
                mime_type: String::new(),
                format: None,
            }
        ))
        .is_durable()
    );
    assert!(!AgentEvent::ModelDone(sampling_result_for_test()).is_durable());
    assert!(
        !AgentEvent::System(SystemEvent::SessionStarted {
            session_id: "s".into(),
        })
        .is_durable()
    );
    assert!(
        !AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed,
        })
        .is_durable()
    );
    assert!(
        !AgentEvent::System(SystemEvent::Progress {
            message: "x".into(),
            step: 1,
            total: 1,
        })
        .is_durable()
    );
    assert!(
        !AgentEvent::System(SystemEvent::Warning {
            kind: WarningKind::Hook,
            message: "x".into(),
            iteration: None,
        })
        .is_durable()
    );
    assert!(
        !AgentEvent::Hook(HookEvent::Message {
            priority: 1,
            message: "x".into(),
        })
        .is_durable()
    );

    // Agent(meta, inner) inherits from inner.
    let durable_inner = AgentEvent::Model(ContentPart::Text(TextContent {
        text: "hi".into(),
        cache_control: None,
    }));
    let ephemeral_inner =
        AgentEvent::Model(ContentPart::Reasoning(ReasoningContent {
            text: "x".into(),
            signature: None,
        }));
    let meta = AgentMeta::new("parent", "child", 0);
    assert!(
        AgentEvent::Agent(meta.clone(), Box::new(durable_inner)).is_durable()
    );
    assert!(!AgentEvent::Agent(meta, Box::new(ephemeral_inner)).is_durable());
}
