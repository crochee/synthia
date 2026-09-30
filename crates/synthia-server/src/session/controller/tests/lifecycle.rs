//! State-machine transitions: concurrent prompt dedup,
//! idle-timeout shutdown, post-shutdown error contract,
//! `OperationSnapshot` bus, idempotent `close()`.

use std::{
    sync::{Arc, Mutex, atomic::AtomicUsize},
    time::Duration,
};

use super::{
    super::{SessionOp, SessionState},
    support::{
        BlockingFactory,
        VecFactory,
        make_manager_and_controller,
        wait_for_runs,
    },
};

#[tokio::test]
async fn test_two_concurrent_prompts_spawn_one_run() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn super::super::RunStreamFactory> =
        Arc::new(BlockingFactory {
            calls: Arc::clone(&calls),
            progress_count: Arc::new(AtomicUsize::new(0)),
        });
    let (controller, manager, _temp) =
        make_manager_and_controller(Duration::from_secs(60), factory).await;

    controller
        .submit(SessionOp::Prompt {
            content: "hello".to_string(),
            priority: 1,
        })
        .await
        .unwrap();

    // Wait until the run is definitely active before
    // submitting the second prompt — synchronized on the
    // factory invocation (see `wait_for_runs`).
    wait_for_runs(&calls, 1).await;

    controller
        .submit(SessionOp::Prompt {
            content: "world".to_string(),
            priority: 1,
        })
        .await
        .unwrap();

    // Wait until the controller has processed the second
    // prompt (observable as a queued input) — a fixed
    // sleep could read `calls` before the op was even
    // handled, which is what made this test racy.
    tokio::time::timeout(Duration::from_secs(2), async {
        while !manager.input_queue().has_pending("alice", "s1").await {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("second prompt was not queued in time");

    assert_eq!(calls.lock().unwrap().len(), 1);

    controller.cancel().await.unwrap();
    tokio::time::timeout(Duration::from_millis(500), async {
        while controller.state() != SessionState::Idle
            && controller.state() != SessionState::Cancelled
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_shutdown_after_idle_timeout() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn super::super::RunStreamFactory> =
        Arc::new(VecFactory::new(vec![], Arc::clone(&calls), None));
    let (controller, _manager, _temp) =
        make_manager_and_controller(Duration::from_millis(50), factory).await;

    assert!(controller.is_alive());
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert!(!controller.is_alive());
}

/// After `idle_timeout` fires and the controller shuts
/// down, a new `submit()` call MUST return
/// `Err` with the documented "session controller is
/// shut down" context — NOT silently drop the op, NOT
/// panic. This is the contract the controller
/// depends on: when `get_or_create_session_controller`
/// fails (because the prior controller just shut down
/// between the call and the `submit`), the error
/// path bubbles up and the chat client sees a 5xx
/// instead of an empty 200 OK.
///
/// Without this contract, a previously-shut-down
/// controller could be silently reused as if it were
/// alive, leaking the queued op into a stale run.
#[tokio::test]
async fn test_submit_after_shutdown_returns_error() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn super::super::RunStreamFactory> =
        Arc::new(VecFactory::new(vec![], Arc::clone(&calls), None));
    let (controller, _manager, _temp) =
        make_manager_and_controller(Duration::from_millis(50), factory).await;

    // Wait for idle_timeout to fire and the controller
    // to drop its op_tx receiver.
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert!(!controller.is_alive());

    // Now submit MUST fail.
    let result = controller
        .submit(SessionOp::Prompt {
            content: "after-shutdown".to_string(),
            priority: 1,
        })
        .await;
    assert!(
        result.is_err(),
        "submit after shutdown must return Err, got Ok"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("shut down"),
        "error must include the documented context; got: {err_msg}"
    );
    // No factory calls were made — the post-shutdown
    // op never reached the run loop.
    assert_eq!(calls.lock().unwrap().len(), 0);
}

/// R29-Phase-I: `close()` stops the loop, emits the terminal
/// `lifecycle_shutdown` event, and is idempotent.
#[tokio::test]
async fn close_emits_lifecycle_shutdown_and_is_idempotent() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    // A blocking run so a live run is genuinely in flight
    // when `close()` lands — the shutdown path must cancel
    // it, not just stop an idle loop.
    let factory: Arc<dyn super::super::RunStreamFactory> =
        Arc::new(BlockingFactory {
            calls: Arc::clone(&calls),
            progress_count: Arc::new(AtomicUsize::new(0)),
        });
    let (controller, manager, temp) =
        make_manager_and_controller(Duration::from_secs(300), factory).await;

    controller
        .submit(SessionOp::Prompt {
            content: "long run".to_string(),
            priority: 0,
        })
        .await
        .unwrap();
    wait_for_runs(&calls, 1).await;

    // First close: stops the loop and finalises the log.
    controller.close().await.unwrap();
    // Second close: a no-op that still resolves.
    controller.close().await.unwrap();

    assert!(
        !controller.is_alive(),
        "the controller loop must have exited after close()"
    );

    // The durable log must carry exactly one terminal
    // `lifecycle_shutdown` event.
    let events_path = super::support::find_events_jsonl(temp.path())
        .expect("events.jsonl must exist after close");
    let body = std::fs::read_to_string(&events_path).unwrap();
    let shutdowns = body
        .lines()
        .filter(|l| l.contains("\"type\":\"session_shutdown\""))
        .count();
    assert_eq!(
        shutdowns, 1,
        "exactly one lifecycle_shutdown event must be appended; log:\n{body}"
    );
    // The sink is closed — a further append must fail.
    let sink = manager.sink("alice", "s1");
    assert!(
        sink.append(&serde_json::json!({"x": 1})).await.is_err(),
        "append after close must fail"
    );
}

/// R12 closure: `subscribe_snapshots()` returns the latest
/// snapshot so a late subscriber learns the current state
/// without waiting for the next transition — the property
/// `GET /sessions/{id}/status` depends on.
#[test]
fn subscribe_snapshots_latest_is_initially_none() {
    let bus = synthia::session::SnapshotBus::new();
    let (_rx, latest) = bus.subscribe();
    assert!(latest.is_none());
    let _seq = bus.publish(synthia::session::OperationSnapshot::started(
        "s1", "default", 25,
    ));
    let (_rx2, latest2) = bus.subscribe();
    let snap = latest2.expect("latest after publish");
    assert_eq!(snap.session_id, "s1");
    assert_eq!(snap.agent_name, "default");
    assert_eq!(snap.max_iterations, 25);
}

/// R11: the controller publishes `OperationSnapshot`s on the
/// shared bus at run-state transitions. A subscriber must
/// observe `Running` when a run starts and `Completing` when
/// the factory stream ends — without touching the run task
/// internals.
#[tokio::test]
async fn snapshot_bus_publishes_running_then_completing() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory = Arc::new(VecFactory::new(
        vec![super::super::AgentEvent::text_delta("done")],
        calls.clone(),
        None,
    ));
    let (controller, _manager, _temp) =
        make_manager_and_controller(Duration::from_secs(300), factory).await;

    // Subscribe BEFORE the run so we observe both transitions.
    let (mut rx, latest) = controller.subscribe_snapshots();
    assert!(latest.is_none(), "fresh bus has no snapshot yet");

    controller
        .submit(SessionOp::Prompt {
            content: "hello".to_string(),
            priority: 0,
        })
        .await
        .unwrap();

    // Wait for the factory to fire (run started) then for the
    // snapshots to land on the bus.
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if controller.subscribe_snapshots().1.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("snapshot bus never received the Running publish");

    // Drain the receiver: Running first, then Completing.
    let first = rx.next().await.expect("Running snapshot");
    assert_eq!(
        first.state,
        synthia::session::OperationState::Running { iteration: 0 }
    );
    let second = rx.next().await.expect("Completing snapshot");
    assert_eq!(second.state, synthia::session::OperationState::Completing);
    assert_eq!(first.session_id, "s1");
    assert_eq!(second.session_id, "s1");
}
