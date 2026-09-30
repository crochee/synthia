//! `SessionOp::Cancel` — terminates the active run, is
//! idempotent under repeated calls, and is a safe no-op when
//! submitted before any run has started.

use std::{
    sync::{Arc, Mutex, atomic::AtomicUsize},
    time::Duration,
};

use super::{
    super::{RunStreamFactory, SessionOp, SessionState},
    support::{
        BlockingFactory,
        VecFactory,
        make_manager_and_controller,
        wait_for_runs,
    },
};

#[tokio::test]
async fn test_cancel_terminates_run() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn RunStreamFactory> = Arc::new(BlockingFactory {
        calls: Arc::clone(&calls),
        progress_count: Arc::new(AtomicUsize::new(0)),
    });
    let (controller, manager, _temp) =
        make_manager_and_controller(Duration::from_secs(60), factory).await;

    controller
        .submit(SessionOp::Prompt {
            content: "start".to_string(),
            priority: 1,
        })
        .await
        .unwrap();

    // Deterministic start signal: the factory invocation,
    // not `state() == Running` (see `wait_for_runs`).
    wait_for_runs(&calls, 1).await;

    // Drain the input queue so the controller does not restart the
    // run immediately after cancellation.
    let _ = manager.input_queue().drain_pending("alice", "s1").await;
    controller.cancel().await.unwrap();

    tokio::time::timeout(Duration::from_millis(500), async {
        while controller.state() != SessionState::Cancelled
            && controller.state() != SessionState::Idle
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();

    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn test_repeated_cancel_is_idempotent() {
    // Verify that submitting multiple `Cancel` ops in
    // quick succession does not panic, double-fire any
    // state transitions, or leave the controller in an
    // unexpected state. `CancellationToken::cancel()` is
    // documented as idempotent and the handler must
    // preserve that property.
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn RunStreamFactory> = Arc::new(BlockingFactory {
        calls: Arc::clone(&calls),
        progress_count: Arc::new(AtomicUsize::new(0)),
    });
    let (controller, manager, _temp) =
        make_manager_and_controller(Duration::from_secs(60), factory).await;
    controller
        .submit(SessionOp::Prompt {
            content: "start".to_string(),
            priority: 1,
        })
        .await
        .unwrap();
    // Deterministic start signal: the factory invocation,
    // not `state() == Running` (see `wait_for_runs`).
    wait_for_runs(&calls, 1).await;
    let _ = manager.input_queue().drain_pending("alice", "s1").await;

    // Three cancels back-to-back — none must error.
    controller.cancel().await.unwrap();
    controller.cancel().await.unwrap();
    controller.cancel().await.unwrap();

    // The state still converges (either `Cancelled` or
    // `Idle` after the controller reaps the run).
    tokio::time::timeout(Duration::from_millis(500), async {
        loop {
            let s = controller.state();
            if s == SessionState::Cancelled || s == SessionState::Idle {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();

    // Exactly one run was ever started (no spurious
    // restarts from repeated cancels re-queueing work).
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn test_cancel_before_run_starts_is_safe_noop() {
    // Edge case: cancel arrives BEFORE any Prompt has
    // been dequeued (e.g. the user submitted then
    // immediately gave up). `run_cancel` is `None` at
    // that point, so the controller must treat the
    // cancel as a safe no-op — no panic, no
    // double-transition, no hang.
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn RunStreamFactory> =
        Arc::new(VecFactory::new(vec![], Arc::clone(&calls), None));
    let (controller, _manager, _temp) =
        make_manager_and_controller(Duration::from_secs(60), factory).await;
    // No Prompt submitted — controller is Idle.
    assert_eq!(controller.state(), SessionState::Idle);
    // Cancel while Idle.
    controller.cancel().await.unwrap();
    // Must remain Idle (no run to cancel).
    assert_eq!(controller.state(), SessionState::Idle);
    // No factory calls were ever made.
    assert_eq!(calls.lock().unwrap().len(), 0);
}
