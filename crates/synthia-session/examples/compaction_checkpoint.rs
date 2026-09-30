//! Durable compaction checkpoint: a splice that survives a restart.
//!
//! Seam shown: `CompactionCheckpoint` bridges the summarising
//! manager's `CompactionRecordView` — in-memory message indices plus
//! the call ids of the shadowed tool results — to a durable
//! `SessionEvent::Compaction` row, but **only** when the run's
//! `SurfaceLedger` can prove where those rows sit in the log and
//! which seqs identify them. The writer feeds that ledger every row
//! it appends, with the row's 1-based log ordinal (`RunLog::append`
//! in `synthia-server`, which is private); this example mirrors it
//! over the crate's own `InMemorySessionSink`, with no agent, no
//! provider call and no network.
//!
//! Run:
//!
//! ```text
//! cargo run -p synthia-session --example compaction_checkpoint
//! ```
//!
//! Look at:
//!
//! 1. the `compaction` row's `surface_op`: the span and cited seqs
//!    are the log's, not the manager's indices (the manager counts
//!    the system prompt the loop never logs, so it reports index 3
//!    where the durable surface holds the row at 2);
//! 2. the fold before vs after: the tool-result row is *replaced*,
//!    never replayed, and the surface keeps its length;
//! 3. the fail-soft run: a call id the log never carried writes no
//!    `compaction` row at all, yet the lifecycle rows still land
//!    and the surface still folds.

use std::sync::Arc;

use serde_json::Value;
use synthia_provider::{Content, Message};
use synthia_session::{
    CompactionCheckpoint,
    CompactionLifecycleView,
    CompactionOutcome,
    CompactionRecordView,
    FoldedSurface,
    SessionEvent,
    SessionSink,
    SurfaceLedger,
    TypedEventReceiver,
    TypedEventSink,
    fold_log_surface,
    in_memory::InMemorySessionSink,
    try_fold_log_surface,
};

/// One run's writer over the session sink.
///
/// Mirrors `RunLog` in `synthia-server/src/session/controller.rs`:
/// append the row to the sink, then feed the run's ledger with the
/// 1-based ordinal the reader will reproduce — the source of truth a
/// checkpoint resolves its provenance against.
struct RunWriter {
    sink: InMemorySessionSink,
    ledger: Arc<SurfaceLedger>,
    next_seq: u64,
}

impl RunWriter {
    /// Wrap the sink, seeded with the ordinal of the last row it
    /// already holds.
    async fn new(
        sink: InMemorySessionSink,
        ledger: Arc<SurfaceLedger>,
    ) -> Self {
        let next_seq = sink
            .snapshot()
            .await
            .map(|snapshot| snapshot.last_event_seq)
            .unwrap_or(0);
        Self {
            sink,
            ledger,
            next_seq,
        }
    }

    /// Append one row, then index it.
    async fn append(&mut self, mut row: Value) {
        if let Some(object) = row.as_object_mut() {
            // The server also stamps a wall-clock `ts`; only the
            // ordinal takes part in provenance.
            object.insert("seq".to_string(), Value::from(self.next_seq + 1));
        }
        self.sink
            .append(&row)
            .await
            .expect("the in-memory sink accepts appends while open");
        self.next_seq += 1;
        self.ledger.record(self.next_seq, &row);
    }

    /// The rows as the resume path reads them.
    async fn rows(&self) -> Vec<Value> {
        self.sink
            .read()
            .await
            .expect("the in-memory sink reads while open")
    }
}

/// The run's rows: a prompt, one assistant line, and the tool result
/// the compaction will shadow.
///
/// Each row is the typed event the controller persists, serialised
/// to its JSONL form. The tool result carries the call id — the only
/// durable identity a record can cite.
fn run_rows() -> Vec<Value> {
    let user = SessionEvent::UserMessage {
        seq: 0,
        ts: String::new(),
        data: serde_json::to_value(Message::user("read the config file"))
            .expect("a user message serialises"),
        surface_op: None,
    };
    let assistant = SessionEvent::AssistantMessage {
        seq: 0,
        ts: String::new(),
        data: serde_json::to_value(Message::assistant("reading it now"))
            .expect("an assistant message serialises"),
        surface_op: None,
    };
    let tool_result = SessionEvent::ToolResult {
        seq: 0,
        ts: String::new(),
        data: serde_json::to_value(Message::tool(
            Content::text("raw output of call-1"),
            "call-1",
        ))
        .expect("a tool result serialises"),
        surface_op: None,
    };
    [user, assistant, tool_result]
        .into_iter()
        .map(|event| {
            serde_json::to_value(&event).expect("an event row serialises")
        })
        .collect()
}

/// Drive the manager's two callbacks for one attempt: the lifecycle
/// triple, then the record.
fn attempt(
    checkpoint: &Arc<CompactionCheckpoint>,
    token: &str,
    record: CompactionRecordView,
) {
    let lifecycle = checkpoint.lifecycle_callback();
    lifecycle(CompactionLifecycleView::Start {
        token: token.to_string(),
    });
    lifecycle(CompactionLifecycleView::Summary {
        token: token.to_string(),
        summary: record.summary_text.clone(),
        shadowed_positions: record
            .source_indices
            .iter()
            .map(|index| *index as u64)
            .collect(),
    });
    checkpoint.record_callback()(record);
    lifecycle(CompactionLifecycleView::End {
        token: token.to_string(),
        outcome: CompactionOutcome::Committed,
    });
}

/// The run-end flush, in the server's order: drain the typed channel
/// into the log, then retry the records that parked and append what
/// resolved. Returns `(parked, resolved-and-written)`.
async fn flush(
    checkpoint: &Arc<CompactionCheckpoint>,
    receiver: &mut TypedEventReceiver,
    log: &mut RunWriter,
) -> (usize, usize) {
    while let Ok(Some(record)) = receiver.try_recv() {
        log.append(record.as_value()).await;
    }
    let parked = checkpoint.pending_len();
    let resolved = checkpoint.resolve_pending();
    let written = resolved.len();
    for event in resolved {
        log.append(
            serde_json::to_value(&event).expect("an event row serialises"),
        )
        .await;
    }
    (parked, written)
}

/// The first row whose `type` tag is `tag`.
fn find_row<'a>(rows: &'a [Value], tag: &str) -> Option<&'a Value> {
    rows.iter()
        .find(|row| row.get("type").and_then(Value::as_str) == Some(tag))
}

/// How many rows carry the `type` tag `tag`.
fn count_rows(rows: &[Value], tag: &str) -> usize {
    rows.iter()
        .filter(|row| row.get("type").and_then(Value::as_str) == Some(tag))
        .count()
}

/// A one-line preview of a folded message: its first text (or
/// summary) string, else the raw JSON, elided to fit the transcript.
fn preview(message: &Value) -> String {
    /// Depth-first search for the first string under `keys`.
    fn find(value: &Value, keys: [&str; 2]) -> Option<String> {
        match value {
            Value::Object(map) => {
                for key in keys {
                    if let Some(Value::String(text)) = map.get(key) {
                        return Some(text.clone());
                    }
                }
                map.values().find_map(|value| find(value, keys))
            }
            Value::Array(items) => {
                items.iter().find_map(|value| find(value, keys))
            }
            _ => None,
        }
    }
    let text = find(message, ["text", "summary"]).unwrap_or_else(|| {
        serde_json::to_string(message).expect("a message serialises")
    });
    let mut out: String = text.chars().take(48).collect();
    if text.chars().count() > 48 {
        out.push('…');
    }
    out
}

/// One line per folded message: surface seq, role, short preview.
fn print_surface(label: &str, folded: &FoldedSurface) {
    println!(
        "{label}: {} message(s), surface_seqs={:?}",
        folded.messages.len(),
        folded.surface_seqs,
    );
    for (index, (message, seq)) in
        folded.messages.iter().zip(&folded.surface_seqs).enumerate()
    {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("<none>");
        println!("  #{index} seq={seq} role={role} {}", preview(message));
    }
}

/// True when any folded message carries `needle`.
fn mentions(folded: &FoldedSurface, needle: &str) -> bool {
    folded.messages.iter().any(|message| {
        serde_json::to_string(message)
            .expect("a message serialises")
            .contains(needle)
    })
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("== run 1: a record the ledger can prove ==");
    let ledger = Arc::new(SurfaceLedger::new());
    let sink = InMemorySessionSink::new("checkpoint-proven");
    let mut log = RunWriter::new(sink, Arc::clone(&ledger)).await;
    for row in run_rows() {
        log.append(row).await;
    }
    let rows_before = log.rows().await;
    println!(
        "run rows: {} (tool result `call-1` at seq 3)",
        rows_before.len()
    );
    let before = fold_log_surface(&rows_before);
    print_surface("surface before the checkpoint", &before);

    let (sink, mut receiver) = TypedEventSink::channel(16);
    let checkpoint = CompactionCheckpoint::new(sink, Arc::clone(&ledger));

    // What the manager reports: it counts the system prompt the
    // loop prepends but never logs, so the tool result is its index
    // 3 while the durable surface holds it at 2. `source_keys`
    // carries the identity the checkpoint actually trusts.
    attempt(
        &checkpoint,
        "t1",
        CompactionRecordView {
            start: 3,
            end: 4,
            source_indices: vec![3],
            source_keys: vec![Some("call-1".to_string())],
            summary_text: "summary: read the config file -> ok".to_string(),
        },
    );
    println!(
        "records parked after the callback: {}",
        checkpoint.pending_len()
    );

    let (parked, written) = flush(&checkpoint, &mut receiver, &mut log).await;
    println!("flush: parked={parked} resolved-and-written={written}");

    let rows_after = log.rows().await;
    let compaction = find_row(&rows_after, "compaction")
        .expect("the checkpoint wrote a compaction row");
    let tag = compaction
        .get("type")
        .and_then(Value::as_str)
        .expect("the row carries a type tag");
    let op = compaction
        .get("surface_op")
        .expect("a compaction row carries a surface_op");
    println!(
        "compaction row: type={tag} seq={} surface_op={}",
        compaction.get("seq").expect("the writer stamped a seq"),
        serde_json::to_string(op).expect("the surface_op serialises"),
    );
    println!(
        "  manager index 3 -> start={} end={} source_event_seqs={}",
        op.get("start").expect("a replace carries a start"),
        op.get("end").expect("a replace carries an end"),
        op.get("source_event_seqs")
            .expect("a replace cites source seqs"),
    );

    let after = fold_log_surface(&rows_after);
    print_surface("surface after the checkpoint", &after);
    let strict = try_fold_log_surface(&rows_after)
        .expect("the checkpoint's log folds under the strict variant");
    assert_eq!(
        strict.surface_seqs, after.surface_seqs,
        "the lenient and strict folds must agree"
    );
    println!("strict fold agrees with the lenient fold");

    assert!(
        mentions(&before, "raw output of call-1"),
        "the pre-compaction log replays the tool result"
    );
    assert!(
        !mentions(&after, "raw output of call-1"),
        "the checkpoint must replace the tool result, not replay it"
    );
    assert!(
        mentions(&after, "summary: read the config file"),
        "the summary took the shadowed row's place"
    );
    assert_eq!(
        before.messages.len(),
        after.messages.len(),
        "a replace swaps one surface slot for one summary"
    );
    println!(
        "tool result in surface: before={} after={} (replaced, not \
         replayed)",
        mentions(&before, "raw output of call-1"),
        mentions(&after, "raw output of call-1"),
    );

    println!();
    println!("== run 2: a record the ledger cannot prove ==");
    let ghost_ledger = Arc::new(SurfaceLedger::new());
    let ghost_sink = InMemorySessionSink::new("checkpoint-unproven");
    let mut ghost_log =
        RunWriter::new(ghost_sink, Arc::clone(&ghost_ledger)).await;
    for row in run_rows() {
        ghost_log.append(row).await;
    }
    let ghost_keys = vec![Some("call-ghost".to_string())];
    println!(
        "ledger lookup for `call-ghost`: {}",
        match ghost_ledger.resolve(&ghost_keys) {
            Ok(_) => "resolved".to_string(),
            Err(gap) => gap.to_string(),
        }
    );

    let (ghost_sink, mut ghost_receiver) = TypedEventSink::channel(16);
    let ghost_checkpoint =
        CompactionCheckpoint::new(ghost_sink, Arc::clone(&ghost_ledger));
    // A call id no row carries: citing it would invent provenance,
    // so the record is parked instead of written.
    attempt(
        &ghost_checkpoint,
        "t2",
        CompactionRecordView {
            start: 3,
            end: 4,
            source_indices: vec![3],
            source_keys: ghost_keys,
            summary_text: "summary: cannot be proven".to_string(),
        },
    );
    println!(
        "records parked after the callback: {}",
        ghost_checkpoint.pending_len()
    );
    let (parked, written) =
        flush(&ghost_checkpoint, &mut ghost_receiver, &mut ghost_log).await;
    println!(
        "flush: parked={parked} resolved-and-written={written} \
         (an unprovable record is dropped)"
    );
    let ghost_rows = ghost_log.rows().await;
    println!(
        "compaction rows in the log: {}",
        count_rows(&ghost_rows, "compaction")
    );
    println!(
        "lifecycle rows written: start={} summary={} end={}",
        count_rows(&ghost_rows, "compaction_start"),
        count_rows(&ghost_rows, "compaction_summary"),
        count_rows(&ghost_rows, "compaction_end"),
    );
    assert_eq!(count_rows(&ghost_rows, "compaction"), 0);
    assert_eq!(count_rows(&ghost_rows, "compaction_start"), 1);
    assert_eq!(count_rows(&ghost_rows, "compaction_summary"), 1);
    assert_eq!(count_rows(&ghost_rows, "compaction_end"), 1);

    let ghost_surface = fold_log_surface(&ghost_rows);
    print_surface("surface after the fail-soft run", &ghost_surface);
    assert_eq!(
        ghost_surface.messages.len(),
        3,
        "no replace was applied; the pre-compaction rows still fold"
    );
    assert!(mentions(&ghost_surface, "raw output of call-1"));
    assert!(
        try_fold_log_surface(&ghost_rows).is_ok(),
        "the surface stays foldable"
    );
    println!("strict fold still succeeds: the surface stays foldable");

    println!("COMPACTION-CHECKPOINT: OK");
}
