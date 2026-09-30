//! Durable compaction checkpoint: bridge the summarising manager's
//! emitters to the typed session log.
//!
//! `SummarizingContextManager` fires two synchronous callbacks — a
//! **record** per splice ([`CompactionRecordView`]) and a
//! **lifecycle** triple per attempt ([`CompactionLifecycleView`]) —
//! and this module is the production end of both:
//!
//! - the lifecycle callback writes `compaction_start` /
//!   `compaction_summary` / `compaction_end` rows immediately, so a
//!   crash between start and end stays detectable
//!   ([`crate::repair::orphaned_compactions`]);
//! - the record callback writes the `SessionEvent::Compaction`
//!   `Replace` **only** when the run's [`SurfaceLedger`] can prove
//!   where the shadowed rows sit and which seqs identify them. A
//!   record whose provenance cannot be established is *not* written:
//!   the compaction is then recorded log-only (the lifecycle rows
//!   already landed) and the surface stays foldable instead of
//!   pointing at the wrong span.
//!
//! ## Why a ledger, not the record's indices
//!
//! The manager reports positions in the *in-memory* message list,
//! which is not the surface: the system prompt is prepended by the
//! loop and never logged, and one assistant turn can span several
//! log rows. The only honest mapping goes through the log itself —
//! each replaced tool result is located by its call id among the rows
//! the run actually appended. That is what [`SurfaceLedger`] indexes.
//!
//! ## Deferred resolution
//!
//! The manager fires from the agent's task; the controller appends
//! rows from its own task, and the agent can run ahead. A record
//! emitted in that window parks, and
//! [`CompactionCheckpoint::resolve_pending`] retries it once the run's
//! rows are all in the log (the server flushes at run end). Parked
//! records are resolved in order, so a second compaction in the same
//! run sees the first one's replacement already applied.

use std::{collections::VecDeque, sync::Arc};

use parking_lot::Mutex;
use serde_json::json;

use crate::{
    events::{CompactionOutcome, SessionEvent},
    log_surface::{MappingGap, ResolvedReplace, SurfaceLedger},
    typed_event_sink::{
        CompactionRecordView,
        TypedEventRecord,
        TypedEventSink,
    },
};

/// Mirrored view of `synthia_context::CompactionLifecycle`.
///
/// `synthia-session` does not depend on `synthia-context`, so the
/// adapter that owns both types converts one into the other — same
/// decoupling as [`CompactionRecordView`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactionLifecycleView {
    /// The manager is about to call the summariser.
    Start {
        /// Correlation id for this lifecycle.
        token: String,
    },
    /// The summariser returned usable text.
    Summary {
        /// Correlation id.
        token: String,
        /// The summary text.
        summary: String,
        /// Pre-splice positions the summary shadows (in-memory
        /// message indices; informational, the record carries the
        /// durable identity).
        shadowed_positions: Vec<u64>,
    },
    /// The lifecycle finished.
    End {
        /// Correlation id.
        token: String,
        /// Terminal state.
        outcome: CompactionOutcome,
    },
}

impl CompactionLifecycleView {
    /// Build the typed log row for this step.
    ///
    /// `seq` and `ts` stay zero/empty — the writer stamps them at
    /// append time, like every other builder in this crate.
    #[must_use]
    pub fn to_typed_event(&self) -> SessionEvent {
        match self {
            Self::Start { token } => SessionEvent::CompactionStart {
                seq: 0,
                ts: String::new(),
                token: token.clone(),
                data: json!({}),
            },
            Self::Summary {
                token,
                summary,
                shadowed_positions,
            } => SessionEvent::CompactionSummary {
                seq: 0,
                ts: String::new(),
                token: token.clone(),
                data: json!({
                    "summary": summary,
                    "shadowed_positions": shadowed_positions,
                }),
            },
            Self::End { token, outcome } => SessionEvent::CompactionEnd {
                seq: 0,
                ts: String::new(),
                token: token.clone(),
                outcome: outcome.as_str().to_string(),
                data: json!({}),
            },
        }
    }
}

/// Bridges a [`CompactionRecordView`] / [`CompactionLifecycleView`]
/// pair to a [`TypedEventSink`].
///
/// Install the two callbacks on a `SummarizingContextManager`:
///
/// ```ignore
/// let checkpoint = CompactionCheckpoint::new(sink, ledger);
/// let manager = SummarizingContextManager::new(summarise)
///     .with_compaction_emitter(checkpoint.record_callback())
///     .with_compaction_lifecycle_emitter(checkpoint.lifecycle_callback());
/// ```
pub struct CompactionCheckpoint {
    sink: TypedEventSink,
    ledger: Arc<SurfaceLedger>,
    /// Records whose shadowed rows were not in the ledger when they
    /// were emitted. Retried by [`Self::resolve_pending`], in order.
    pending: Mutex<VecDeque<CompactionRecordView>>,
}

impl CompactionCheckpoint {
    /// Build a checkpoint over `sink` and the run's `ledger`.
    #[must_use]
    pub fn new(sink: TypedEventSink, ledger: Arc<SurfaceLedger>) -> Arc<Self> {
        Arc::new(Self {
            sink,
            ledger,
            pending: Mutex::new(VecDeque::new()),
        })
    }

    /// The run's surface ledger. The writer feeds it every appended
    /// row; the checkpoint reads it.
    #[must_use]
    pub fn ledger(&self) -> &Arc<SurfaceLedger> {
        &self.ledger
    }

    /// Callback for
    /// `SummarizingContextManager::with_compaction_emitter`.
    #[must_use]
    pub fn record_callback(
        self: &Arc<Self>,
    ) -> Arc<dyn Fn(CompactionRecordView) + Send + Sync> {
        let checkpoint = Arc::clone(self);
        Arc::new(move |view: CompactionRecordView| {
            checkpoint.emit_record(view);
        })
    }

    /// Callback for
    /// `SummarizingContextManager::with_compaction_lifecycle_emitter`.
    #[must_use]
    pub fn lifecycle_callback(
        self: &Arc<Self>,
    ) -> Arc<dyn Fn(CompactionLifecycleView) + Send + Sync> {
        let sink = self.sink.clone();
        Arc::new(move |step: CompactionLifecycleView| {
            sink.record(TypedEventRecord::new(step.to_typed_event()));
        })
    }

    /// Resolve every parked record, in emission order, and return the
    /// `SessionEvent::Compaction` rows the caller must append.
    ///
    /// Call this once the run has appended all of its rows (the
    /// server does, right after draining the typed channel). A record
    /// that still cannot be mapped is dropped with a warning naming
    /// the gap: the compaction stays log-only and the surface stays
    /// foldable.
    ///
    /// The caller MUST append the returned rows through the same
    /// writer that feeds [`Self::ledger`], so a later parked record
    /// sees the earlier replacement applied.
    #[must_use]
    pub fn resolve_pending(&self) -> Vec<SessionEvent> {
        let parked = {
            let mut pending = self.pending.lock();
            std::mem::take(&mut *pending)
        };
        let mut events = Vec::with_capacity(parked.len());
        for view in parked {
            match self.resolve(&view) {
                Ok(event) => events.push(event),
                Err(gap) => {
                    tracing::warn!(
                        target: "synthia.session",
                        %gap,
                        "compaction checkpoint: recording log-only, the \
                         shadowed rows cannot be proven; the surface is \
                         left foldable"
                    );
                }
            }
        }
        events
    }

    /// Number of records still waiting for their rows to reach the
    /// log.
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.lock().len()
    }

    /// Resolve now, or park and warn.
    fn emit_record(&self, view: CompactionRecordView) {
        match self.resolve(&view) {
            Ok(event) => self.sink.record(TypedEventRecord::new(event)),
            Err(gap) => {
                tracing::warn!(
                    target: "synthia.session",
                    %gap,
                    "compaction checkpoint: shadowed rows not in the durable \
                     log yet; deferring the record to the run-end flush"
                );
                self.pending.lock().push_back(view);
            }
        }
    }

    /// Map the record's keys onto the ledger and build the event.
    fn resolve(
        &self,
        view: &CompactionRecordView,
    ) -> Result<SessionEvent, MappingGap> {
        let expected = view.end.saturating_sub(view.start);
        if expected != view.source_keys.len() {
            return Err(MappingGap::CountMismatch {
                expected,
                found: view.source_keys.len(),
            });
        }
        let resolved = self.ledger.resolve(&view.source_keys)?;
        Ok(event_for(view, &resolved))
    }
}

/// Build the durable `Compaction` row from a resolved span.
fn event_for(
    view: &CompactionRecordView,
    resolved: &ResolvedReplace,
) -> SessionEvent {
    SessionEvent::Compaction {
        seq: 0,
        ts: String::new(),
        surface_op: crate::events::SurfaceOp::Replace {
            start: resolved.start,
            end: resolved.end,
            source_event_seqs: resolved.source_event_seqs.clone(),
        },
        data: json!({
            "source_indices": view.source_indices,
            "summary": view.summary_text,
        }),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::{
        events::SurfaceOp,
        fold_log_surface,
        fold_surface,
        repair::{compaction_outcomes, orphaned_compactions},
        surface_events_from_log,
        typed_event_sink::TypedEventReceiver,
    };

    /// The production log shape: a user prompt, one assistant text
    /// row, then two tool results, each with a durable call id.
    fn log_rows() -> Vec<Value> {
        vec![
            json!({"type": "UserInput", "data": {"text": "hello"}}),
            json!({"type": "Model", "data": {"type": "text", "text": "working"}}),
            tool_result_row("c1"),
            tool_result_row("c2"),
        ]
    }

    fn tool_result_row(call_id: &str) -> Value {
        json!({
            "type": "Model",
            "data": {
                "type": "tool_result",
                "tool_use_id": call_id,
                "tool_name": "read",
                "content": [{
                    "type": "text",
                    "text": format!("raw output of {call_id}"),
                }],
                "structured_content": null,
                "is_error": null,
            }
        })
    }

    /// A record replacing `keys`, using the in-memory indices a real
    /// manager would report for the tool batch.
    fn record(keys: &[&str], summary: &str) -> CompactionRecordView {
        CompactionRecordView {
            start: 2,
            end: 2 + keys.len(),
            source_indices: (2..2 + keys.len()).collect(),
            source_keys: keys.iter().map(|k| Some((*k).to_string())).collect(),
            summary_text: summary.to_string(),
        }
    }

    /// Feed a log into a ledger exactly as a run writer would:
    /// every row, in order, with its 1-based ordinal.
    fn feed(ledger: &SurfaceLedger, rows: &[Value]) {
        for (index, row) in rows.iter().enumerate() {
            ledger.record(index as u64 + 1, row);
        }
    }

    fn drain(receiver: &mut TypedEventReceiver) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        while let Ok(Some(record)) = receiver.try_recv() {
            events.push(record.event);
        }
        events
    }

    /// Run one full lifecycle through the checkpoint's callbacks.
    fn run_lifecycle(
        checkpoint: &Arc<CompactionCheckpoint>,
        view: CompactionRecordView,
    ) {
        let record = checkpoint.record_callback();
        let lifecycle = checkpoint.lifecycle_callback();
        lifecycle(CompactionLifecycleView::Start {
            token: "t1".to_string(),
        });
        lifecycle(CompactionLifecycleView::Summary {
            token: "t1".to_string(),
            summary: view.summary_text.clone(),
            shadowed_positions: view
                .source_indices
                .iter()
                .map(|i| *i as u64)
                .collect(),
        });
        record(view);
        lifecycle(CompactionLifecycleView::End {
            token: "t1".to_string(),
            outcome: CompactionOutcome::Committed,
        });
    }

    #[test]
    fn checkpoint_writes_lifecycle_and_resolved_replace() {
        let rows = log_rows();
        let ledger = Arc::new(SurfaceLedger::new());
        feed(&ledger, &rows);
        let (sink, mut receiver) = TypedEventSink::channel(16);
        let checkpoint = CompactionCheckpoint::new(sink, Arc::clone(&ledger));

        run_lifecycle(&checkpoint, record(&["c1", "c2"], "compacted tail"));

        let events = drain(&mut receiver);
        assert_eq!(events.len(), 4, "start, summary, record, end: {events:?}");
        let SessionEvent::CompactionStart { token, .. } = &events[0] else {
            panic!("expected start, got {:?}", events[0]);
        };
        assert_eq!(token, "t1");
        let SessionEvent::CompactionSummary { token, data, .. } = &events[1]
        else {
            panic!("expected summary, got {:?}", events[1]);
        };
        assert_eq!(token, "t1");
        assert_eq!(data["summary"], "compacted tail");
        let SessionEvent::Compaction {
            surface_op, data, ..
        } = &events[2]
        else {
            panic!("expected the compaction record, got {:?}", events[2]);
        };
        // The written span and cited seqs are the *log's*, not the
        // manager's in-memory indices: rows 3 and 4 are the two tool
        // results, and they sit at surface positions 2 and 3.
        let SurfaceOp::Replace {
            start,
            end,
            source_event_seqs,
        } = surface_op
        else {
            panic!("compaction must carry a replace op");
        };
        assert_eq!((*start, *end), (2, 4));
        assert_eq!(source_event_seqs, &vec![3_u64, 4]);
        assert_eq!(data["summary"], "compacted tail");
        let SessionEvent::CompactionEnd { token, outcome, .. } = &events[3]
        else {
            panic!("expected end, got {:?}", events[3]);
        };
        assert_eq!(token, "t1");
        assert_eq!(outcome, "committed");
        assert_eq!(checkpoint.pending_len(), 0);
    }

    #[test]
    fn folded_written_log_shows_the_compacted_surface() {
        let rows = log_rows();
        let ledger = Arc::new(SurfaceLedger::new());
        feed(&ledger, &rows);
        let (sink, mut receiver) = TypedEventSink::channel(16);
        let checkpoint = CompactionCheckpoint::new(sink, Arc::clone(&ledger));
        run_lifecycle(&checkpoint, record(&["c1", "c2"], "compacted tail"));

        // The written log is the run's rows plus the record the
        // checkpoint emitted, in append order.
        let mut written: Vec<Value> = rows;
        let events = drain(&mut receiver);
        let compaction = events
            .iter()
            .find(|event| matches!(event, SessionEvent::Compaction { .. }))
            .expect("the checkpoint wrote a record");
        written
            .push(serde_json::to_value(compaction).expect("record serialises"));

        let folded = fold_surface(&surface_events_from_log(&written))
            .expect("the written log folds");
        assert_eq!(
            folded.surface_seqs,
            vec![1, 2, 5],
            "the shadowed tool rows are gone: {folded:?}"
        );
        assert_eq!(folded.messages.len(), 3);
        assert_eq!(folded.messages[0]["role"], "user");
        assert_eq!(folded.messages[1]["role"], "assistant");
        assert_eq!(folded.messages[2]["summary"], "compacted tail");
        let serialized = serde_json::to_string(&folded.messages)
            .expect("surface serialises");
        assert!(
            !serialized.contains("raw output of c1")
                && !serialized.contains("raw output of c2"),
            "pre-compaction rows must not survive the fold: {serialized}"
        );

        // The lenient raw entry point agrees with the typed fold.
        let lenient = fold_log_surface(&written);
        assert_eq!(lenient.surface_seqs, folded.surface_seqs);
    }

    #[test]
    fn closed_lifecycle_is_paired_and_an_open_one_is_interrupted() {
        let rows = log_rows();
        let ledger = Arc::new(SurfaceLedger::new());
        feed(&ledger, &rows);
        let (sink, mut receiver) = TypedEventSink::channel(16);
        let checkpoint = CompactionCheckpoint::new(sink, Arc::clone(&ledger));
        run_lifecycle(&checkpoint, record(&["c1", "c2"], "compacted"));
        let closed = drain(&mut receiver);
        assert!(orphaned_compactions(&closed).is_empty());
        assert_eq!(
            compaction_outcomes(&closed),
            vec![("t1".to_string(), CompactionOutcome::Committed)]
        );

        // A crash between start and end leaves the start unmatched.
        let (sink, mut receiver) = TypedEventSink::channel(16);
        let checkpoint = CompactionCheckpoint::new(sink, Arc::clone(&ledger));
        checkpoint.lifecycle_callback()(CompactionLifecycleView::Start {
            token: "t2".to_string(),
        });
        let interrupted = drain(&mut receiver);
        assert_eq!(
            compaction_outcomes(&interrupted),
            vec![("t2".to_string(), CompactionOutcome::Interrupted)]
        );
        let orphans = orphaned_compactions(&interrupted);
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].token, "t2");
    }

    #[test]
    fn unresolvable_mapping_writes_no_checkpoint() {
        // The ledger knows rows 1..2 only — the tool rows are not
        // there, so nothing may be written for them.
        let rows = log_rows();
        let ledger = Arc::new(SurfaceLedger::new());
        feed(&ledger, &rows[..2]);
        let (sink, mut receiver) = TypedEventSink::channel(16);
        let checkpoint = CompactionCheckpoint::new(sink, Arc::clone(&ledger));

        run_lifecycle(
            &checkpoint,
            record(&["c1", "c2"], "must not be written"),
        );
        assert_eq!(checkpoint.pending_len(), 1, "the record parks");

        let events = drain(&mut receiver);
        assert!(
            events
                .iter()
                .all(|event| !matches!(event, SessionEvent::Compaction { .. })),
            "no compaction row may be written without provenance: {events:?}"
        );
        assert_eq!(events.len(), 3, "start + summary + end: {events:?}");

        // Still unresolvable at the flush: the record is dropped,
        // and the surface stays foldable.
        assert!(checkpoint.resolve_pending().is_empty());
        assert_eq!(checkpoint.pending_len(), 0);

        let mut log = rows.clone();
        let folded = fold_log_surface(&log);
        assert_eq!(folded.messages.len(), 4, "no replace was applied");

        // A keyless row is refused the same way.
        let view = CompactionRecordView {
            start: 2,
            end: 3,
            source_indices: vec![2],
            source_keys: vec![None],
            summary_text: "no key".to_string(),
        };
        checkpoint.record_callback()(view);
        assert_eq!(checkpoint.pending_len(), 1);
        assert!(checkpoint.resolve_pending().is_empty());

        // Sanity: the log is still the pre-compaction one.
        log.push(
            json!({"type": "Model", "data": {"type": "text", "text": "tail"}}),
        );
        assert_eq!(fold_log_surface(&log).messages.len(), 5);
    }

    #[test]
    fn deferred_records_resolve_once_their_rows_land() {
        let rows = log_rows();
        let ledger = Arc::new(SurfaceLedger::new());
        let (sink, mut receiver) = TypedEventSink::channel(16);
        let checkpoint = CompactionCheckpoint::new(sink, Arc::clone(&ledger));

        // Emitted before the writer caught up.
        run_lifecycle(&checkpoint, record(&["c1", "c2"], "late"));
        assert_eq!(checkpoint.pending_len(), 1);

        // The writer appends the run's rows after the fact.
        feed(&ledger, &rows);

        let events = checkpoint.resolve_pending();
        assert_eq!(events.len(), 1);
        let SessionEvent::Compaction { surface_op, .. } = &events[0] else {
            panic!("expected the resolved record");
        };
        let SurfaceOp::Replace {
            start,
            end,
            source_event_seqs,
        } = surface_op
        else {
            panic!("compaction must carry a replace op");
        };
        assert_eq!((*start, *end), (2, 4));
        assert_eq!(source_event_seqs, &vec![3_u64, 4]);
        assert_eq!(checkpoint.pending_len(), 0);

        let events = drain(&mut receiver);
        assert_eq!(events.len(), 3, "the lifecycle rows were never parked");
    }

    #[test]
    fn a_second_compaction_sees_the_first_replacement_applied() {
        let ledger = Arc::new(SurfaceLedger::new());
        let mut rows = log_rows();
        rows.push(tool_result_row("c3"));
        feed(&ledger, &rows);
        let (sink, mut receiver) = TypedEventSink::channel(16);
        let checkpoint = CompactionCheckpoint::new(sink, Arc::clone(&ledger));

        // First compaction shadows c1 + c2.
        checkpoint.record_callback()(record(&["c1", "c2"], "first"));
        // The writer appends the record it was handed.
        let first = drain(&mut receiver)
            .into_iter()
            .find(|event| matches!(event, SessionEvent::Compaction { .. }))
            .expect("first record");
        let written = serde_json::to_value(&first).expect("serialises");
        ledger.record(rows.len() as u64 + 1, &written);

        // Second compaction shadows c3, which now sits one row earlier.
        checkpoint.record_callback()(CompactionRecordView {
            start: 4,
            end: 5,
            source_indices: vec![4],
            source_keys: vec![Some("c3".to_string())],
            summary_text: "second".to_string(),
        });
        let second = drain(&mut receiver)
            .into_iter()
            .find(|event| matches!(event, SessionEvent::Compaction { .. }))
            .expect("second record");
        let SessionEvent::Compaction { surface_op, .. } = &second else {
            panic!("expected a compaction row");
        };
        let SurfaceOp::Replace {
            start,
            end,
            source_event_seqs,
        } = surface_op
        else {
            panic!("compaction must carry a replace op");
        };
        assert_eq!(
            (*start, *end),
            (3, 4),
            "the first replacement removed a slot before c3"
        );
        assert_eq!(source_event_seqs, &vec![5_u64]);
    }

    #[test]
    fn an_unresolvable_replace_in_the_log_is_ignored_on_read() {
        // A hand-written log citing a seq that does not exist must not
        // break the resume: the span is replayed instead.
        let mut rows = log_rows();
        rows.push(json!({
            "type": "compaction",
            "ts": "",
            "surface_op": {"start": 2, "end": 4, "source_event_seqs": [99]},
            "data": {"summary": "bogus"},
        }));
        let folded = fold_log_surface(&rows);
        assert_eq!(folded.messages.len(), 4, "the bogus op is ignored");
        assert_eq!(folded.surface_seqs, vec![1, 2, 3, 4]);
    }

    /// A tool result that carried an image must replay with the image
    /// intact: the raw row's `content` array holds the parts, and the
    /// log fold copies them through the message decoder rather than
    /// reducing them to text. This is the durable half of the
    /// multimodal chain — a screenshot a tool returned in turn 2 is
    /// still there when the session is resumed from disk.
    #[test]
    fn a_tool_result_image_row_replays_with_its_payload() {
        let rows = vec![
            json!({"type": "UserInput", "data": {"text": "screenshot it"}}),
            json!({
                "type": "Model",
                "data": {
                    "type": "tool_result",
                    "tool_use_id": "c_img",
                    "tool_name": "screenshot",
                    "content": [
                        {"type": "text", "text": "captured"},
                        {
                            "type": "image",
                            "data": "iVBORw0KGgoAAAANSUhEUg==",
                            "mime_type": "image/png",
                            "detail": null,
                        }
                    ],
                    "structured_content": null,
                    "is_error": null,
                }
            }),
        ];
        let folded = fold_log_surface(&rows);
        assert_eq!(folded.messages.len(), 2, "both rows are surface-eligible");
        // A tool result is a `Content::Single(ToolResult)` whose *inner*
        // `content` array holds the parts — that is where the image
        // lives on the wire.
        let content = folded.messages[1]["content"]["Single"]["content"]
            .as_array()
            .unwrap_or_else(|| {
                panic!(
                    "expected an inner content array: {}",
                    folded.messages[1]
                )
            });
        assert_eq!(content.len(), 2, "text + image: {content:?}");
        assert_eq!(content[0]["text"], "captured");
        assert_eq!(content[1]["type"], "image");
        assert_eq!(content[1]["data"], "iVBORw0KGgoAAAANSUhEUg==");
        assert_eq!(content[1]["mime_type"], "image/png");
        // The payload decodes back into the provider `Message` the
        // rehydration path consumes — the image survives the round
        // trip, not just the serialization.
        let message: synthia_provider::Message =
            serde_json::from_value(folded.messages[1].clone())
                .expect("folded payload decodes as a Message");
        // The image is nested inside the tool result's own content.
        let image = message
            .content
            .iter()
            .find_map(|p| match p {
                synthia_provider::ContentPart::ToolResult(tr) => {
                    tr.content.iter().find_map(|inner| match inner {
                        synthia_provider::ContentPart::Image(img) => Some(img),
                        _ => None,
                    })
                }
                _ => None,
            })
            .expect("decoded tool result must still carry the image");
        assert_eq!(image.data, "iVBORw0KGgoAAAANSUhEUg==");
        assert_eq!(image.mime_type, "image/png");
        // The strict fold agrees — an image row is not a violation.
        assert!(crate::try_fold_log_surface(&rows).is_ok());
    }

    /// A typed `user_message` row carrying a multi-part content
    /// array must replay with its image intact.
    ///
    /// This pins the **fold's** part-preservation contract, not a
    /// controller write: the controller's own user-prompt rows are
    /// text-only by design (see
    /// `test_prompt_multi_persists_its_text_as_a_user_input_row` in
    /// `synthia-server`, which asserts image bytes never reach the
    /// sink). The `user_message` shape arrives from the typed
    /// `SessionEvent` layer, which normalises a bare `content`
    /// array into `ContentPart`s — and a normaliser that silently
    /// keeps only text parts is a real bug class (the same one this
    /// round fixed in `synthia-web`'s `toolResultContentText`).
    #[test]
    fn a_user_message_image_row_replays_with_its_payload() {
        let rows = vec![json!({
            "type": "user_message",
            "seq": 1,
            "data": {
                "role": "user",
                "content": [
                    {"type": "text", "text": "what is this?"},
                    {
                        "type": "image",
                        "data": "QUJD",
                        "mime_type": "image/jpeg",
                        "detail": "auto",
                    }
                ],
            }
        })];
        let folded = fold_log_surface(&rows);
        let content = folded.messages[0]["content"]["Multi"]
            .as_array()
            .unwrap_or_else(|| {
                panic!("expected a Multi content array: {}", folded.messages[0])
            });
        assert_eq!(content.len(), 2);
        assert_eq!(content[1]["type"], "image");
        assert_eq!(content[1]["mime_type"], "image/jpeg");
        let message: synthia_provider::Message =
            serde_json::from_value(folded.messages[0].clone())
                .expect("folded payload decodes as a Message");
        assert!(
            message
                .content
                .iter()
                .any(|p| matches!(p, synthia_provider::ContentPart::Image(_)))
        );
    }
}
