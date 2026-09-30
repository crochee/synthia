//! Persist + broadcast of agent events.
//!
//! Pins the durable / ephemeral event split (after the
//! panel/session refactor, only `durable` events land in the
//! JSONL sink; system markers broadcast to subscribers but do
//! not get persisted) and the R4 Phase C.3 `request_header`
//! epoch marker (emitted once per `(provider, model,
//! tools_hash)` tuple).

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use synthia::{
    harness::{SessionEndReason, SystemEvent},
    provider::{ContentPart, TextContent},
};

use super::{
    super::{AgentEvent, SessionOp, SessionState},
    support::{VecFactory, make_manager_and_controller},
};

#[tokio::test]
async fn test_events_are_persisted_and_broadcast() {
    // MVP: all events are durable. Persist every event the
    // factory emits, then verify both broadcast and persistence
    // observed them.
    let events = vec![
        AgentEvent::Model(ContentPart::Text(TextContent {
            text: "hi".to_string(),
            cache_control: None,
        })),
        AgentEvent::System(SystemEvent::SessionStarted {
            session_id: "s1".to_string(),
        }),
        AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed,
        }),
    ];
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn super::super::RunStreamFactory> =
        Arc::new(VecFactory::new(events, Arc::clone(&calls), None));
    let (controller, manager, _temp) =
        make_manager_and_controller(Duration::from_secs(60), factory).await;

    // Subscribe before the run starts so events are broadcast.
    let mut rx = controller.subscribe();

    controller
        .submit(SessionOp::Prompt {
            content: "go".to_string(),
            priority: 1,
        })
        .await
        .unwrap();

    // Wait for the run to start.
    tokio::time::timeout(Duration::from_millis(500), async {
        while calls.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();

    // Drain the queue so the controller does not immediately restart
    // the run, then wait for the run to finish.
    let _ = manager.input_queue().drain_pending("alice", "s1").await;
    tokio::time::timeout(Duration::from_millis(500), async {
        while controller.state() != SessionState::Idle {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();

    let persisted: Vec<serde_json::Value> =
        manager.sink("alice", "s1").read().await.unwrap();
    let type_of = |e: &serde_json::Value| -> String {
        e.get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    // After the panel/session refactor, the sink stores
    // only **durable** events (per the
    // `event-durability-classification` spec). System
    // events such as `SessionStarted` / `SessionEnded` are
    // ephemeral: they are broadcast to subscribers but not
    // persisted, because a cold start rebuilds the agent
    // state from the durable slices (model text, tool
    // calls, tool results, resources). The run task
    // also appends a synthetic `UserInput` envelope at
    // run start (after reading the sink, before invoking
    // the agent) so the NEXT run can reconstruct
    // user-role messages; that is durable too. Since R4
    // Phase C.3 the FIRST run of a session also stamps a
    // typed `request_header` epoch marker before the
    // `UserInput` (dsh `EpochHeader` semantics), so the
    // JSONL must contain exactly three records: the
    // `request_header`, this run's `UserInput`, and the
    // `Model(Text)` from the agent.
    assert_eq!(
        persisted.len(),
        3,
        "durable events + typed request_header are persisted; got {persisted:?}"
    );
    assert_eq!(type_of(&persisted[0]), "request_header");
    assert_eq!(
        persisted[0]["data"]["reason"], "initial",
        "first run must be an initial epoch"
    );

    assert!(
        persisted.iter().any(|e| type_of(e) == "UserInput"),
        "UserInput envelope from the prompt handler should be persisted"
    );
    assert!(
        persisted
            .iter()
            .any(|e| type_of(e) == "Model" || type_of(e) == "model"),
        "Model event should be persisted"
    );

    // Second run on the same controller with unchanged deps
    // MUST NOT re-emit the `request_header` (only `change`
    // or `initial` re-stamp it — dsh dedup semantics).
    controller
        .submit(SessionOp::Prompt {
            content: "again".to_string(),
            priority: 1,
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_millis(500), async {
        while controller.state() != SessionState::Idle {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let persisted2: Vec<serde_json::Value> =
        manager.sink("alice", "s1").read().await.unwrap();
    let header_count = persisted2
        .iter()
        .filter(|e| type_of(e) == "request_header")
        .count();
    assert_eq!(
        header_count, 1,
        "unchanged config must not re-emit request_header; got {persisted2:?}"
    );

    let received = tokio::time::timeout(Duration::from_millis(200), rx.recv())
        .await
        .unwrap()
        .unwrap();
    // The first broadcast event is the Message event (emitted first
    // by the factory).
    assert!(matches!(received, AgentEvent::Model(ContentPart::Text(_))));
}
