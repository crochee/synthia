//! The process-wide usage counters behind `/api/v1/chat/usage`.
//!
//! The counters are written by the controller's single event funnel
//! (`persist_and_broadcast`), so the contract is asserted from the
//! outside: real agent events are driven through a real controller and
//! the injected [`UsageMetrics`] handle is read back — never the
//! harness internals.

use std::{
    sync::{Arc, Mutex, atomic::AtomicUsize},
    time::Duration,
};

use synthia::harness::{SessionEndReason, SystemEvent};

use super::{
    super::{AgentEvent, RunStreamFactory, SessionOp, SessionState},
    support::{
        BlockingFactory,
        VecFactory,
        make_manager_and_controller_with_deps,
        test_deps,
        wait_for_runs,
    },
};
use crate::state::UsageMetrics;

/// A run that reports token usage and then ends must move both token
/// buckets by the reported values and count exactly one turn. The two
/// buckets are asserted separately so an in/out swap fails here.
#[tokio::test]
async fn test_usage_and_session_end_events_reach_the_counters() {
    let metrics = Arc::new(UsageMetrics::default());

    let events = vec![
        AgentEvent::System(SystemEvent::Usage {
            input_tokens: 1_024,
            output_tokens: 256,
            cache_read_tokens: None,
            cache_creation_tokens: None,
        }),
        AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed,
        }),
    ];
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn RunStreamFactory> =
        Arc::new(VecFactory::new(events, Arc::clone(&calls), None));
    let deps = test_deps().with_usage_metrics(Arc::clone(&metrics));
    let (controller, manager, _temp) = make_manager_and_controller_with_deps(
        Duration::from_secs(60),
        factory,
        deps,
    )
    .await;

    controller
        .submit(SessionOp::Prompt {
            content: "go".to_string(),
            priority: 1,
        })
        .await
        .unwrap();
    wait_for_runs(&calls, 1).await;

    // Drain the queue so the controller does not immediately restart
    // the run, then wait for `Idle` — the run task publishes it only
    // after every event of the stream was folded.
    let _ = manager.input_queue().drain_pending("alice", "s1").await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while controller.state() != SessionState::Idle {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();

    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.tokens_in, 1_024);
    assert_eq!(snapshot.tokens_out, 256);
    assert_eq!(snapshot.turns, 1);
}

/// A cancelled run still consumed a turn — `turns` counts finished
/// runs, not successful ones. The run reports no `Usage` at all, so
/// the token buckets must stay put while `turns` moves.
#[tokio::test]
async fn test_cancelled_run_still_counts_a_turn() {
    let metrics = Arc::new(UsageMetrics::default());

    let calls = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn RunStreamFactory> = Arc::new(BlockingFactory {
        calls: Arc::clone(&calls),
        progress_count: Arc::new(AtomicUsize::new(0)),
    });
    let deps = test_deps().with_usage_metrics(Arc::clone(&metrics));
    let (controller, _manager, _temp) = make_manager_and_controller_with_deps(
        Duration::from_secs(60),
        factory,
        deps,
    )
    .await;

    controller
        .submit(SessionOp::Prompt {
            content: "go".to_string(),
            priority: 1,
        })
        .await
        .unwrap();
    wait_for_runs(&calls, 1).await;

    controller.cancel().await.unwrap();

    // `Cancel` flips the state synchronously, before the run task has
    // drained the `SessionEnded { Cancelled }` that closes its stream,
    // so the counter — not `state()` — is the completion signal here.
    tokio::time::timeout(Duration::from_secs(2), async {
        while metrics.snapshot().turns == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the cancelled run never recorded its turn");

    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.turns, 1);
    assert_eq!(snapshot.tokens_in, 0);
    assert_eq!(snapshot.tokens_out, 0);
}
