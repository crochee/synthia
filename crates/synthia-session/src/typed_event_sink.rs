//! [`TypedEventSink`] — typed-event bridge between the agent
//! runtime and the persisted session log.
//!
//! ## Background
//!
//! R5 introduced the typed [`SessionEvent`] enum and the
//! `interrupted_turn_closers` repair helpers, but did **not** add
//! any producers for the structural variants
//! (`Compaction` / `Usage` / `Iteration` / `Step` / `Turn` /
//! `SubagentEnter` / `SubagentExit` / `RequestHeader`). R6-3
//! changes that by exposing a thin sink the agent loop can hold
//! alongside its in-memory `StreamSink`; events go to the mpsc
//! channel, the controller drains them, and each event lands as
//! one line in the JSONL log via `SessionSink::append`.
//!
//! ## Runtime neutrality
//!
//! The channel is `futures::channel::mpsc` — **not** tokio's —
//! so this module's public API (`TypedEventSink` /
//! `TypedEventReceiver`) stays runtime-agnostic. A consumer on
//! any async runtime (or none, using `try_next`) drains the
//! receiver; the agent side only ever calls the synchronous
//! `record`.
//!
//! ## Wiring
//!
//! The agent loop constructs one `TypedEventSink` per run and
//! hands it down to:
//!
//! - `SummarizingContextManager::with_compaction_emitter` and
//!   `with_compaction_lifecycle_emitter` through
//!   [`CompactionCheckpoint`](crate::CompactionCheckpoint), which
//!   resolves each splice against the run's
//!   [`SurfaceLedger`](crate::SurfaceLedger) before anything is
//!   written
//! - the structural-boundary emitters (`iteration_start/end`,
//!   `step_start/end`, `request_header`) added by R6-A
//!
//! The mpsc receiver lives inside the controller. Each event
//! round-trip is fire-and-forget; if the channel is full the
//! sink drops the event rather than blocking the hot path
//! (callers should size the buffer with headroom).
//!
//! ## Why not just call `SessionSink::append` directly?
//!
//! Three reasons:
//!
//! 1. The agent loop runs on a single task; persisting each
//!    event synchronously serialises on the file lock.
//! 2. The typed event schema (`SessionEvent::Compaction`) carries
//!    semantics the JSONL writer needs to know about
//!    (`surface_op::Replace` + `source_event_seqs`); coupling
//!    the manager to the sink would force every implementation
//!    to re-derive that structure.
//! 3. lib consumers that wire their own run factory (R6 = the
//!    "lego round") get one stable type to hold instead of
//!    chasing `Arc<dyn SessionSink>`.

use futures::{StreamExt, channel::mpsc};
use serde_json::Value;

use crate::events::SessionEvent;

/// One typed event ready for persistence. Wrapped to let callers
/// pre-stamp `seq` and `ts` once, at the sink boundary, rather
/// than at every producer site.
#[derive(Debug, Clone)]
pub struct TypedEventRecord {
    /// Pre-built typed event. `seq` and `ts` should be `0` /
    /// `""`; the sink stamps them on emission.
    pub event: SessionEvent,
}

impl TypedEventRecord {
    /// Wrap a pre-built typed event for emission.
    #[must_use]
    pub fn new(event: SessionEvent) -> Self {
        Self { event }
    }

    /// Materialise as the JSON value the JSONL writer accepts.
    /// Stamps `seq: 0` and an empty `ts` — the controller is
    /// responsible for re-stamping with the live seq once it
    /// has appended; this default shape round-trips through
    /// `SessionEvent::from_value` so callers that ignore the
    /// seq are still correct.
    #[must_use]
    pub fn as_value(&self) -> Value {
        serde_json::to_value(&self.event).unwrap_or(Value::Null)
    }
}

/// Mirrored view of a `SummarizingContextManager` `CompactionRecord`.
///
/// The manager's `CompactionRecord` is defined in
/// `synthia-context::summarizing`; this mirror keeps the two crates
/// decoupled — synthia-session does not depend on synthia-context.
///
/// The record's `start`/`end` are positions in the manager's
/// **in-memory** message list, which is not the durable surface (the
/// loop prepends a system prompt that is never logged, and one
/// assistant turn can span several log rows). They are therefore
/// *not* what gets written: [`CompactionCheckpoint`](crate::CompactionCheckpoint)
/// locates the shadowed rows through
/// [`source_keys`](Self::source_keys) in the run's
/// [`SurfaceLedger`](crate::SurfaceLedger) and writes the span and
/// seqs the durable fold will reproduce. A record whose keys do not
/// resolve is recorded log-only rather than written with provenance
/// it cannot back.
#[derive(Debug, Clone)]
pub struct CompactionRecordView {
    /// Inclusive start index in the pre-splice `messages` vector.
    pub start: usize,
    /// Exclusive end index in the pre-splice `messages` vector.
    pub end: usize,
    /// Index range being replaced (`messages[start..end]`).
    pub source_indices: Vec<usize>,
    /// Durable identity of each replaced message, parallel to
    /// `source_indices`: the tool-call id of a tool-result message.
    /// `None` for a message with no durable key — a checkpoint
    /// cannot be proven for it.
    pub source_keys: Vec<Option<String>>,
    /// Summary text the manager produced.
    pub summary_text: String,
}

/// Sender side of the typed-event channel. Cheap to clone (the
/// inner `futures::channel::mpsc::Sender` already is). Producers
/// hold an `Arc` to this; the controller holds the receiver.
///
/// The channel is deliberately **not** tokio's: the public API
/// of this crate stays runtime-neutral (R7 objective).
#[derive(Clone)]
pub struct TypedEventSink {
    tx: mpsc::Sender<TypedEventRecord>,
}

impl TypedEventSink {
    /// Build a sink + receiver pair with the given channel
    /// capacity. The controller holds the receiver and drains
    /// events into the JSONL log.
    #[must_use]
    pub fn channel(capacity: usize) -> (TypedEventSink, TypedEventReceiver) {
        let (tx, rx) = mpsc::channel(capacity.max(1));
        (TypedEventSink { tx }, TypedEventReceiver { rx })
    }

    /// Build a sink from a pre-existing sender (e.g. when the
    /// controller hands a clone down to the agent run task).
    #[must_use]
    pub fn from_sender(tx: mpsc::Sender<TypedEventRecord>) -> Self {
        Self { tx }
    }

    /// Send one record. Returns immediately; if the channel is
    /// full or the receiver was dropped the event is dropped
    pub fn record(&self, record: TypedEventRecord) {
        // `try_send` — never block the agent loop. futures'
        // `Sender::try_send` takes `&mut self`, so clone the
        // cheap (Arc-backed) sender for this call.
        let mut tx = self.tx.clone();
        let _ = tx.try_send(record);
    }

    /// Clone the inner sender. Used by tests that want to drive
    /// the channel directly.
    #[must_use]
    pub fn sender(&self) -> mpsc::Sender<TypedEventRecord> {
        self.tx.clone()
    }
}

/// Receiver side of the typed-event channel. The controller
/// drains it and appends each event to the JSONL log via
/// `SessionSink::append` (stamping `seq` / `ts` at write time).
///
/// `recv` is runtime-neutral (`futures::StreamExt::next`), and
/// `try_recv` serves the controller's post-stream synchronous
/// drain: after the agent run's event stream ends, every record
/// the loop published is already buffered, so one `try_recv`
/// loop flushes them without spawning a task or awaiting
/// channel closure.
pub struct TypedEventReceiver {
    rx: mpsc::Receiver<TypedEventRecord>,
}

impl TypedEventReceiver {
    /// Wrap a pre-existing receiver.
    #[must_use]
    pub fn new(rx: mpsc::Receiver<TypedEventRecord>) -> Self {
        Self { rx }
    }

    /// Await one record. Returns `None` when every sink was
    /// dropped (the agent run finished or was cancelled).
    pub async fn recv(&mut self) -> Option<TypedEventRecord> {
        self.rx.next().await
    }

    /// Non-blocking receive: `Ok(Some(record))` when buffered,
    /// `Ok(None)` when the channel is closed and empty,
    /// `Err(TryRecvError)` when the buffer is momentarily empty
    /// but the channel is still open.
    pub fn try_recv(
        &mut self,
    ) -> Result<Option<TypedEventRecord>, futures::channel::mpsc::TryRecvError>
    {
        match self.rx.try_recv() {
            Ok(record) => Ok(Some(record)),
            // Channel closed and drained — surface as `Ok(None)`
            // so the controller's drain loop terminates cleanly.
            Err(err) if err.is_closed() => Ok(None),
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sink_drops_records_when_receiver_gone() {
        // When the controller has gone away the sink must not
        // panic on send; producers are fire-and-forget.
        let (sink, rx) = TypedEventSink::channel(8);
        drop(rx);
        sink.record(TypedEventRecord::new(SessionEvent::Warning {
            seq: 0,
            ts: String::new(),
            data: serde_json::json!({"kind": "orphaned"}),
        }));
    }

    #[tokio::test]
    async fn sink_drains_records_in_order() {
        let (sink, mut receiver) = TypedEventSink::channel(8);
        for i in 0..4 {
            sink.record(TypedEventRecord::new(SessionEvent::Warning {
                seq: 0,
                ts: String::new(),
                data: serde_json::json!({"kind": format!("kind_{i}")}),
            }));
        }
        for i in 0..4 {
            let rec = receiver.recv().await.expect("drain");
            let SessionEvent::Warning { data, .. } = rec.event else {
                panic!("wrong variant");
            };
            assert_eq!(data["kind"], format!("kind_{i}"));
        }
    }

    #[tokio::test]
    async fn try_recv_flushes_buffered_then_reports() {
        // The controller's post-stream drain contract: every
        // record published before the sink was dropped is
        // buffered, so a `try_recv` loop flushes them all and
        // then reports closure (`Ok(None)`).
        let (sink, mut receiver) = TypedEventSink::channel(8);
        for i in 0..3 {
            sink.record(TypedEventRecord::new(SessionEvent::Step {
                seq: 0,
                ts: String::new(),
                data: serde_json::json!({"step": i}),
            }));
        }
        drop(sink);
        let mut seen = 0;
        loop {
            match receiver.try_recv() {
                Ok(Some(_)) => seen += 1,
                Ok(None) => break,
                Err(_) => break,
            }
        }
        assert_eq!(seen, 3);
    }
}
