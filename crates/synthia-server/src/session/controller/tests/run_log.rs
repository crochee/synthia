//! `RunLog` ledger-feed and ordinal provenance.
//!
//! The run's writer is the only thing that can tell a compaction
//! checkpoint where its shadowed rows are: it feeds the ledger
//! with the row ordinal the reader will reproduce, and it stamps
//! that same ordinal on typed rows.
use std::sync::Arc;

use synthia::session::{SessionSink, SurfaceLedger};

use super::super::run_log::RunLog;

#[tokio::test]
async fn run_log_feeds_the_ledger_with_log_ordinals() {
    let sink =
        Arc::new(synthia::session::in_memory::InMemorySessionSink::new("s1"));
    let ledger = Arc::new(SurfaceLedger::new());
    let mut log = RunLog::new(
        sink.clone(),
        Arc::clone(&ledger),
        0,
        synthia::core::SharedClock::system(),
    );

    log.append(&serde_json::json!({
        "type": "UserInput",
        "data": {"text": "hi"},
    }))
    .await
    .unwrap();
    log.append(&serde_json::json!({
        "type": "Model",
        "data": {
            "type": "tool_result",
            "tool_use_id": "c1",
            "content": [{"type": "text", "text": "out"}],
        }
    }))
    .await
    .unwrap();

    assert_eq!(ledger.len(), 2);
    let resolved = ledger
        .resolve(&[Some("c1".to_string())])
        .expect("the tool row resolves");
    assert_eq!(resolved.start, 1, "the tool row is the second surface row");
    assert_eq!(resolved.source_event_seqs, vec![2]);

    // A typed row is stamped with the ordinal it will have in the
    // log, which is what a checkpoint cites.
    let event = synthia::session::SessionEvent::CompactionEnd {
        seq: 0,
        ts: String::new(),
        token: "t".to_string(),
        outcome: "failed".to_string(),
        data: serde_json::json!({}),
    };
    log.append_typed(serde_json::to_value(&event).unwrap())
        .await
        .unwrap();
    let rows = sink.read().await.unwrap();
    assert_eq!(rows[2]["seq"], 3);
    assert!(
        rows[2]["ts"].as_str().is_some_and(|ts| !ts.is_empty()),
        "the wall clock is stamped too"
    );
}

/// The ledger must record the ordinal the **sink** assigned, not
/// `previous + 1`.
///
/// The controller's op loop appends out-of-band — `Feedback`
/// rows, and the shutdown marker, which can land while a run is
/// still writing. A locally counted value would file the next run
/// row one slot too low, and a checkpoint citing it would fail
/// `validate_replace`'s `cited >= self_seq` check: the lenient
/// fold then replays the span the checkpoint had replaced, and
/// the strict fold errors. The sole-appender test above cannot
/// catch that — this one interleaves the other writer.
#[tokio::test]
async fn run_log_takes_its_ordinal_from_the_sink_not_a_local_counter() {
    let sink =
        Arc::new(synthia::session::in_memory::InMemorySessionSink::new("s1"));
    // Seed the writer as the run task does: the ordinal of the
    // last row already on disk.
    sink.append(
        &serde_json::json!({"type": "UserInput", "data": {"text": "old"}}),
    )
    .await
    .unwrap();
    let ledger = Arc::new(SurfaceLedger::new());
    let mut log = RunLog::new(
        sink.clone(),
        Arc::clone(&ledger),
        1,
        synthia::core::SharedClock::system(),
    );

    log.append(&serde_json::json!({
        "type": "UserInput",
        "data": {"text": "run row"},
    }))
    .await
    .unwrap();

    // The other writer appends between two of the run's own.
    sink.append(&serde_json::json!({
        "kind": "feedback",
        "message_id": "m1",
        "thumbs_up": true,
    }))
    .await
    .unwrap();

    log.append(&serde_json::json!({
        "type": "Model",
        "data": {
            "type": "tool_result",
            "tool_use_id": "c9",
            "content": [{"type": "text", "text": "out"}],
        }
    }))
    .await
    .unwrap();

    let rows = sink.read().await.unwrap();
    assert_eq!(rows.len(), 4, "seed + run + feedback + run");
    // The tool row is physically row 4. The ledger must say so —
    // a local counter would have said 3 (the feedback row's
    // ordinal) and made any checkpoint citing it unresolvable.
    let resolved = ledger
        .resolve(&[Some("c9".to_string())])
        .expect("the tool row resolves");
    assert_eq!(
        resolved.source_event_seqs,
        vec![4],
        "the ledger records the sink's ordinal, not previous + 1"
    );
}
