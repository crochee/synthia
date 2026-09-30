//! Multi-turn memory contracts.
//!
//! The second prompt must seed its `history` from the previous
//! run's persisted turns; user-role messages must be
//! reconstructed as `Role::User`; a durable compaction
//! checkpoint written by an earlier run must fold to the
//! compacted surface (summary in place of tool results).

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use synthia::provider::{Content, ContentPart, Role, TextContent};

use super::{
    super::{AgentEvent, SessionOp, SessionState},
    support::{RecordingFactory, make_manager_and_controller},
};

/// Regression: the second prompt submitted to the same session
/// must seed `AgentInput::history` with the assistant turns the
/// previous run persisted to the `SessionSink`.
///
/// Without the fix, the controller never re-reads the sink, so
/// `input.history` is empty on every run and the LLM sees a
/// fresh conversation — losing multi-turn memory.
#[tokio::test]
async fn test_second_prompt_seeds_history_from_persisted_turns() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    // First run emits one durable assistant text chunk; second
    // run emits another. Both must be observable to the agent.
    let factory: Arc<dyn super::super::RunStreamFactory> =
        Arc::new(RecordingFactory::new(
            Arc::clone(&calls),
            vec![AgentEvent::Model(ContentPart::Text(TextContent {
                text: "first reply".to_string(),
                cache_control: None,
            }))],
            vec![AgentEvent::Model(ContentPart::Text(TextContent {
                text: "second reply".to_string(),
                cache_control: None,
            }))],
        ));

    let (controller, _manager, _temp) =
        make_manager_and_controller(Duration::from_secs(60), factory).await;

    // Turn 1
    controller
        .submit(SessionOp::Prompt {
            content: "turn-1 prompt".to_string(),
            priority: 1,
        })
        .await
        .unwrap();

    // Wait for the run to finish and the controller to return
    // to Idle so we know the first run's events have been
    // persisted by `persist_and_broadcast`.
    tokio::time::timeout(Duration::from_secs(2), async {
        while controller.state() != SessionState::Idle {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("first run should finish");

    // After state flips to Idle the controller still holds the
    // first run's `JoinHandle` for one more select! iteration;
    // only once `run_handle.await` returns and sets
    // `run_handle = None` does a new op_rx prompt actually
    // spawn a fresh run. Wait for that window to close before
    // submitting turn 2, otherwise the controller will see
    // `run_handle.is_some()` and drop the dispatch. Poll on
    // the captured calls vector: once it stops growing, the
    // first run's stream has been fully consumed.
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if calls.lock().unwrap().len() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("first run input should be recorded");
    // Tiny grace period for the spawned task to exit so the
    // controller loop clears `run_handle`.
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Turn 2 — submits a fresh prompt. The fix must cause
    // `input.history` of run #2 to contain run #1's assistant
    // text "first reply" so the agent has memory of the prior
    // turn.
    controller
        .submit(SessionOp::Prompt {
            content: "turn-2 prompt".to_string(),
            priority: 1,
        })
        .await
        .unwrap();

    // The controller serializes through op_rx; once the
    // second run is dispatched the factory records a second
    // input. Allow generous slack because the controller may
    // briefly sit in its idle branch before re-entering the
    // select! iteration that picks up op_rx.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if calls.lock().unwrap().len() >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("second run should start");

    let recorded = calls.lock().unwrap();
    assert_eq!(recorded.len(), 2, "controller should have spawned two runs");

    // The second run's input must carry the first run's
    // persisted assistant text inside `history`. We allow
    // the current-turn prompt itself to live in `content`,
    // but the prior assistant turn must be in `history` —
    // that is the contract being violated by the bug.
    let second = &recorded[1];
    let history_text = second
        .history
        .iter()
        .filter_map(|m| match &m.content {
            Content::Single(ContentPart::Text(t)) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        history_text.contains("first reply"),
        "second run's history must include first run's assistant text; got history={:?}",
        second.history
    );

    // The current-turn prompt must live in `content`, NOT in
    // `history` — appending the prompt to the sink before the
    // run drains the queue would cause this assertion to fail
    // because the run would read its own prompt back out of
    // `sink_history` and feed it to the agent twice (once via
    // `history`, once via `content`). The fix persists the
    // drained prompts after reading the sink.
    assert!(
        !history_text.contains("turn-2 prompt"),
        "second run's history must NOT include the current-turn prompt (would cause \
         duplicate-feed); got history={:?}",
        second.history
    );

    // Also assert turn-2's prompt landed as the input content
    // so we know the history seed didn't overwrite the prompt.
    let second_prompt_text = match second.content.first() {
        Some(ContentPart::Text(t)) => t.text.clone(),
        _ => panic!("second run content should be text"),
    };
    assert_eq!(second_prompt_text, "turn-2 prompt");
}

/// Regression: user prompts must be persisted to the session sink
/// and reconstructed as `Message{Role:User}` in the next run's
/// history. Without the `UserInput` envelope append in the
/// `Prompt` handler, `events_to_history` only sees assistant
/// messages and the LLM treats every turn as a fresh conversation.
#[tokio::test]
async fn test_user_prompt_persists_and_reacts_to_role_user_in_history() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn super::super::RunStreamFactory> =
        Arc::new(RecordingFactory::new(
            Arc::clone(&calls),
            vec![AgentEvent::Model(ContentPart::Text(TextContent {
                text: "ack".to_string(),
                cache_control: None,
            }))],
            vec![AgentEvent::Model(ContentPart::Text(TextContent {
                text: "ack".to_string(),
                cache_control: None,
            }))],
        ));

    let (controller, _manager, _temp) =
        make_manager_and_controller(Duration::from_secs(60), factory).await;

    controller
        .submit(SessionOp::Prompt {
            content: "我的猫叫蓝杉".to_string(),
            priority: 1,
        })
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(2), async {
        while controller.state() != SessionState::Idle {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("first run should finish");

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if calls.lock().unwrap().len() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("first run input should be recorded");
    tokio::time::sleep(Duration::from_millis(20)).await;

    controller
        .submit(SessionOp::Prompt {
            content: "它几岁？".to_string(),
            priority: 1,
        })
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if calls.lock().unwrap().len() >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("second run should start");

    let recorded = calls.lock().unwrap();
    assert_eq!(recorded.len(), 2);
    let second = &recorded[1];

    let user_messages: Vec<&str> = second
        .history
        .iter()
        .filter(|m| m.role == Role::User)
        .filter_map(|m| match &m.content {
            Content::Single(ContentPart::Text(t)) => Some(t.text.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        user_messages.iter().any(|t| t.contains("蓝杉")),
        "second run history must contain turn-1 user prompt as Role::User; got history={:?}",
        second.history
    );

    // The current-turn prompt must NOT appear in `history` —
    // duplicating it would make the LLM treat the prompt as a
    // fresh message and discard the conversation context.
    assert!(
        user_messages.iter().all(|t| !t.contains("它几岁")),
        "second run history must NOT include the current-turn prompt (would cause \
         duplicate-feed); got history={:?}",
        second.history
    );
}

/// The run path rebuilds every run's history from the durable log
/// (`events_to_messages` on `session_store.read()`); a compaction
/// checkpoint written by an earlier run must therefore show up as
/// the *compacted* surface — the summary in place of the tool
/// results, not the pre-compaction span. That is the whole point
/// of persisting the checkpoint.
#[tokio::test]
async fn resumed_history_shows_the_durable_compaction_checkpoint() {
    let tool_row = |call_id: &str| {
        serde_json::json!({
            "type": "Model",
            "data": {
                "type": "tool_result",
                "tool_use_id": call_id,
                "tool_name": "read",
                "content": [{
                    "type": "text",
                    "text": format!("raw output of {call_id}"),
                }],
            }
        })
    };

    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn super::super::RunStreamFactory> =
        Arc::new(RecordingFactory::new(
            Arc::clone(&calls),
            vec![AgentEvent::Model(ContentPart::Text(TextContent {
                text: "ack".to_string(),
                cache_control: None,
            }))],
            vec![],
        ));
    let (controller, manager, _temp) =
        make_manager_and_controller(Duration::from_secs(60), factory).await;

    // Prime the log exactly as a compacted run leaves it: rows
    // 1..4 are the turn's messages, row 5 is the checkpoint the
    // run's checkpoint bridge wrote (span + cited log seqs).
    let sink = manager.sink("alice", "s1");
    sink.append(&serde_json::json!({
        "type": "UserInput",
        "data": {"text": "hello"},
    }))
    .await
    .unwrap();
    sink.append(&serde_json::json!({
        "type": "Model",
        "data": {"type": "text", "text": "working"},
    }))
    .await
    .unwrap();
    sink.append(&tool_row("c1")).await.unwrap();
    sink.append(&tool_row("c2")).await.unwrap();
    sink.append(&serde_json::json!({
        "type": "compaction",
        "seq": 5,
        "ts": "2026-09-12T00:00:00Z",
        "surface_op": {"start": 2, "end": 4, "source_event_seqs": [3, 4]},
        "data": {"source_indices": [2, 3], "summary": "compacted tail"},
    }))
    .await
    .unwrap();

    controller
        .submit(SessionOp::Prompt {
            content: "carry on".to_string(),
            priority: 1,
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while calls.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the run factory was not invoked in time");
    let recorded = calls.lock().unwrap();
    let history = &recorded[0].history;
    let flattened = format!("{history:?}");
    assert!(
        flattened.contains("compacted tail"),
        "the checkpoint's summary must seed the resumed history; got {flattened}"
    );
    assert!(
        !flattened.contains("raw output of c1")
            && !flattened.contains("raw output of c2"),
        "the pre-compaction span must not be replayed; got {flattened}"
    );
}
