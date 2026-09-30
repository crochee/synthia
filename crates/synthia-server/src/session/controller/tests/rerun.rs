//! `SessionOp::Rerun` — replays a recovered turn.
//!
//! The rerun's persisted prompt row carries a `SurfaceOp::Replace`
//! citing the previous turn's rows, so the durable log folds to
//! the replacement story (`prompt, answer2`) instead of holding
//! the turn twice. Chained reruns compose (the fold cites the
//! surviving rows, not the long-shadowed original turn); a rerun
//! against a log with no user turn degrades to a plain append;
//! and a rerun arriving while a run is already in flight still
//! starts its own run via the `pending_multimodal` slot.

use std::{
    sync::{Arc, Mutex, atomic::AtomicUsize},
    time::Duration,
};

use synthia::{
    harness::{SessionEndReason, SystemEvent},
    provider::{ContentPart, TextContent},
};

use super::{
    super::{AgentEvent, SessionOp},
    support::{
        BlockingFactory,
        RecordingFactory,
        make_manager_and_controller,
        message_text,
        wait_for_log_row,
        wait_for_runs,
    },
};

/// `SessionOp::Rerun` must hand the supplied parts to the
/// factory through the multimodal seam, so a regenerate actually
/// replays the recovered turn.
///
/// Rerun was unreachable until R72 (the route could not recover a
/// turn from the log), so this pins semantics that are only now
/// live: the parts arrive as the run's input rather than being
/// parked and silently dropped.
#[tokio::test]
async fn rerun_hands_its_parts_to_the_factory() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn super::super::RunStreamFactory> =
        Arc::new(RecordingFactory::new(
            Arc::clone(&calls),
            vec![AgentEvent::System(SystemEvent::SessionEnded {
                reason: SessionEndReason::Completed,
            })],
            vec![],
        ));
    let (controller, _manager, _temp) =
        make_manager_and_controller(Duration::from_secs(60), factory).await;

    let parts = vec![ContentPart::Text(TextContent {
        text: "replay me".to_string(),
        cache_control: None,
    })];
    controller
        .submit(SessionOp::Rerun {
            parts: parts.clone(),
            agent_name: None,
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
    .expect("rerun must start a run");

    let inputs = calls.lock().unwrap();
    let text = inputs[0]
        .content
        .iter()
        .filter_map(|p| match p {
            ContentPart::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(
        text.contains("replay me"),
        "the recovered parts must reach the agent; got {text:?}"
    );
}

/// A rerun's persisted prompt row must shadow the turn it
/// replaces: it carries a `SurfaceOp::Replace` citing the
/// previous turn's rows, so the durable log folds to the
/// replacement story (`prompt, answer2`) instead of holding
/// the turn twice.
///
/// This is the server half of the `/chat/:id` vs
/// `/sessions/:id` regenerate divergence: the fold is the
/// projection behind the next run's history
/// (`events_to_messages`), session search, and the
/// regenerate route's own prompt recovery, so replace
/// semantics here agree everywhere at once.
#[tokio::test]
async fn rerun_prompt_row_shadows_the_turn_it_replaces() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn super::super::RunStreamFactory> =
        Arc::new(RecordingFactory::new(
            Arc::clone(&calls),
            vec![
                AgentEvent::Model(ContentPart::Text(TextContent {
                    text: "fresh answer".to_string(),
                    cache_control: None,
                })),
                AgentEvent::System(SystemEvent::SessionEnded {
                    reason: SessionEndReason::Completed,
                }),
            ],
            vec![],
        ));
    let (controller, manager, _temp) =
        make_manager_and_controller(Duration::from_secs(60), factory).await;

    // Seed the durable log with one completed turn: the
    // prompt at ordinal 1, its answer at ordinal 2.
    let sink = manager.sink("alice", "s1");
    sink.append(&serde_json::json!({
        "type": "UserInput",
        "data": { "text": "original question" },
    }))
    .await
    .unwrap();
    sink.append(&serde_json::json!({
        "type": "Model",
        "data": { "type": "text", "text": "old answer" },
    }))
    .await
    .unwrap();

    controller
        .submit(SessionOp::Rerun {
            parts: vec![ContentPart::Text(TextContent {
                text: "original question".to_string(),
                cache_control: None,
            })],
            agent_name: None,
            priority: 1,
        })
        .await
        .unwrap();

    let rows = wait_for_log_row(&manager, "fresh answer").await;

    // The rerun's prompt row cites the shadowed turn.
    let rerun_row = rows
        .iter()
        .rev()
        .find(|row| {
            row.get("type").and_then(serde_json::Value::as_str)
                == Some("UserInput")
        })
        .expect("the rerun must persist its prompt row");
    let op = serde_json::from_value::<synthia::session::SurfaceOp>(
        rerun_row["surface_op"].clone(),
    )
    .expect("the rerun prompt row carries a surface_op");
    assert_eq!(
        op,
        synthia::session::SurfaceOp::Replace {
            start: 0,
            end: 2,
            source_event_seqs: vec![1, 2],
        },
        "the op must shadow the seeded turn exactly; log: {rows:?}"
    );

    // The fold — the next run's history — shows the
    // replacement story, not the turn twice.
    let folded = synthia::context::events_to_messages(&rows);
    assert_eq!(folded.len(), 2, "one prompt, one answer; log: {rows:?}");
    assert_eq!(message_text(&folded[0]), "original question");
    assert_eq!(message_text(&folded[1]), "fresh answer");

    controller.close().await.unwrap();
}

/// A second rerun must shadow the FIRST rerun's collapsed
/// surface, proving the ordinal math composes: the fold cites
/// the surviving rows (`rerun prompt, new answer`), not the
/// long-shadowed original turn.
#[tokio::test]
async fn chained_rerun_shadows_the_previous_rerun() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn super::super::RunStreamFactory> =
        Arc::new(RecordingFactory::new(
            Arc::clone(&calls),
            vec![
                AgentEvent::Model(ContentPart::Text(TextContent {
                    text: "fresh answer".to_string(),
                    cache_control: None,
                })),
                AgentEvent::System(SystemEvent::SessionEnded {
                    reason: SessionEndReason::Completed,
                }),
            ],
            vec![
                AgentEvent::Model(ContentPart::Text(TextContent {
                    text: "newest answer".to_string(),
                    cache_control: None,
                })),
                AgentEvent::System(SystemEvent::SessionEnded {
                    reason: SessionEndReason::Completed,
                }),
            ],
        ));
    let (controller, manager, _temp) =
        make_manager_and_controller(Duration::from_secs(60), factory).await;

    let sink = manager.sink("alice", "s1");
    sink.append(&serde_json::json!({
        "type": "UserInput",
        "data": { "text": "original question" },
    }))
    .await
    .unwrap();
    sink.append(&serde_json::json!({
        "type": "Model",
        "data": { "type": "text", "text": "old answer" },
    }))
    .await
    .unwrap();

    let rerun_parts = vec![ContentPart::Text(TextContent {
        text: "original question".to_string(),
        cache_control: None,
    })];
    controller
        .submit(SessionOp::Rerun {
            parts: rerun_parts.clone(),
            agent_name: None,
            priority: 1,
        })
        .await
        .unwrap();
    wait_for_log_row(&manager, "fresh answer").await;

    controller
        .submit(SessionOp::Rerun {
            parts: rerun_parts,
            agent_name: None,
            priority: 1,
        })
        .await
        .unwrap();
    let rows = wait_for_log_row(&manager, "newest answer").await;

    let folded = synthia::context::events_to_messages(&rows);
    assert_eq!(
        folded.len(),
        2,
        "a chained rerun still folds to one prompt + one \
         answer; log: {rows:?}"
    );
    assert_eq!(message_text(&folded[0]), "original question");
    assert_eq!(message_text(&folded[1]), "newest answer");

    controller.close().await.unwrap();
}

/// A rerun against a log with no user turn has nothing to
/// shadow; the prompt row must degrade to a plain append
/// (no `surface_op`) rather than citing a span that does not
/// exist. The regenerate route refuses this case up front, so
/// this pins the defensive fallback.
#[tokio::test]
async fn rerun_without_a_user_turn_appends_plainly() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn super::super::RunStreamFactory> =
        Arc::new(RecordingFactory::new(
            Arc::clone(&calls),
            vec![
                AgentEvent::Model(ContentPart::Text(TextContent {
                    text: "orphan answer".to_string(),
                    cache_control: None,
                })),
                AgentEvent::System(SystemEvent::SessionEnded {
                    reason: SessionEndReason::Completed,
                }),
            ],
            vec![],
        ));
    let (controller, manager, _temp) =
        make_manager_and_controller(Duration::from_secs(60), factory).await;

    // No user turn: an assistant row only.
    let sink = manager.sink("alice", "s1");
    sink.append(&serde_json::json!({
        "type": "Model",
        "data": { "type": "text", "text": "answer without a prompt" },
    }))
    .await
    .unwrap();

    controller
        .submit(SessionOp::Rerun {
            parts: vec![ContentPart::Text(TextContent {
                text: "late question".to_string(),
                cache_control: None,
            })],
            agent_name: None,
            priority: 1,
        })
        .await
        .unwrap();

    let rows = wait_for_log_row(&manager, "orphan answer").await;
    let rerun_row = rows
        .iter()
        .rev()
        .find(|row| {
            row.get("type").and_then(serde_json::Value::as_str)
                == Some("UserInput")
        })
        .expect("the rerun must persist its prompt row");
    assert!(
        rerun_row.get("surface_op").is_none(),
        "nothing to shadow; log: {rows:?}"
    );

    controller.close().await.unwrap();
}

/// A `Rerun` that arrives while a run is already in flight must
/// still start its own run. The payload parks in
/// `pending_multimodal` (the text queue stays empty), so a
/// completion gate that only consulted the queue dropped the
/// rerun entirely and left the parts parked for whatever op ran
/// next — which would then answer the stale prompt.
#[tokio::test]
async fn rerun_during_an_active_run_still_starts_its_own_run() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn super::super::RunStreamFactory> =
        Arc::new(BlockingFactory {
            calls: Arc::clone(&calls),
            progress_count: Arc::new(AtomicUsize::new(0)),
        });
    let (controller, _manager, _temp) =
        make_manager_and_controller(Duration::from_secs(60), factory).await;

    // Start a long run.
    controller
        .submit(SessionOp::Prompt {
            content: "original".to_string(),
            priority: 1,
        })
        .await
        .unwrap();
    wait_for_runs(&calls, 1).await;

    // Rerun while that run is still active. The arm cancels the
    // in-flight token, so the blocking run ends and the parked
    // parts are what it should pick up.
    controller
        .submit(SessionOp::Rerun {
            parts: vec![ContentPart::Text(TextContent {
                text: "replayed".to_string(),
                cache_control: None,
            })],
            agent_name: None,
            priority: 1,
        })
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(3), async {
        while calls.lock().unwrap().len() < 2 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect(
        "a rerun submitted mid-run must start its own run; being \
         parked in `pending_multimodal` is not a reason to drop it",
    );

    controller.cancel().await.unwrap();
}
