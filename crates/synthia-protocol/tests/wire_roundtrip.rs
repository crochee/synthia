//! End-to-end wire round-trip tests for all protocol types.
use synthia_protocol::*;

#[test]
fn full_submission_roundtrip() {
    let s = Submission {
        id: SubmissionId::new(),
        op: Op::UserInput {
            items: vec![
                InputItem::Text {
                    text: "summarize".into(),
                },
                InputItem::File {
                    path: "/etc/hosts".into(),
                    content_b64: "AAA=".into(),
                },
            ],
            final_output_json_schema: None,
            additional_context: Some("concise".into()),
        },
        client_user_message_id: Some("msg-42".into()),
        trace: Some(W3cTraceContext {
            traceparent:
                "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01".into(),
            tracestate: None,
        }),
    };
    let j = serde_json::to_string(&s).unwrap();
    let p: Submission = serde_json::from_str(&j).unwrap();
    assert_eq!(p.id, s.id);
    match p.op {
        Op::UserInput { items, .. } => assert_eq!(items.len(), 2),
        _ => panic!("wrong"),
    }
}

#[test]
fn all_op_variants_serialize() {
    let ops: Vec<Op> = vec![
        Op::Interrupt { reason: "x".into() },
        Op::Compact {
            manual: true,
            summary_hint: None,
        },
        Op::ThreadRollback { num_turns: 3 },
        Op::RefreshTools,
        Op::UpdateModel {
            model: "gpt-4".into(),
        },
        Op::UpdateThinkingLevel {
            level: ThinkingLevel::High,
        },
        Op::SwitchSession {
            session_id: SessionId::new(),
        },
        Op::ForkSession {
            at_message_id: MessageId::new(),
        },
    ];
    for op in ops {
        let j = serde_json::to_string(&op).unwrap();
        let _: Op = serde_json::from_str(&j).unwrap();
    }
}

#[test]
fn all_event_variants_serialize() {
    let sid = SessionId::new();
    let ev: Vec<EventMsg> = vec![
        EventMsg::SessionCreated {
            session_id: sid,
            parent_session_id: None,
            cli_version: "0.2.0".into(),
        },
        EventMsg::TurnStarted {
            session_id: sid,
            turn_id: TurnId::new(),
            model: "claude".into(),
        },
        EventMsg::TurnComplete {
            session_id: sid,
            turn_id: TurnId::new(),
            status: TurnStatus::Completed,
        },
        EventMsg::CompactStarted {
            session_id: sid,
            reason: CompactReason::Auto,
            current_tokens: 50000,
            threshold: 100000,
            can_cancel: true,
        },
        EventMsg::CompactCompleted {
            session_id: sid,
            summary: "s".into(),
            dropped_message_ids: vec![],
            new_leaf: MessageId::new(),
        },
        EventMsg::ThreadRolledBack {
            session_id: sid,
            target_message_id: MessageId::new(),
            num_turns: 2,
        },
        EventMsg::TokenCount {
            session_id: sid,
            info: TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
                cached_input_tokens: 80,
            },
        },
        EventMsg::Error {
            session_id: sid,
            kind: "tool".into(),
            payload: serde_json::json!({"d":1}),
            recoverable: false,
        },
    ];
    for e in ev {
        let j = serde_json::to_string(&e).unwrap();
        let _: EventMsg = serde_json::from_str(&j).unwrap();
    }
}
