//! Streaming chunk ingestion for the ReAct loop.
//!
//! Turns the provider's [`StreamChunk`] stream into [`AgentEvent`]s
//! and accumulates the assistant turn.
//!
//! The accumulation itself is **not** re-implemented here: it is
//! [`synthia_provider::BlockAssembler`], the provider crate's tested
//! chunk-to-message fold. This module owns the other half — the
//! *timing* — deciding when an assembled part becomes an
//! [`AgentEvent::Model`] on the wire, and holding the single cursor
//! that keeps a part from being published twice.
//!
//! Before this module used the assembler it carried a second copy of
//! the fold (partial tool buffers, `seen_tool_ids` dedup, the
//! `IsDone` re-list rules) and had drifted from the original in three
//! ways: it dropped reasoning entirely, parsed malformed tool
//! arguments without the JSON repair the provider crate applies, and
//! had no single place that owned the ordering rules.
//!
//! Interface to the parent module: [`StreamSink`] (typed emitters
//! over the event channel) and [`ChunkState`] (the accumulator owned
//! by one sampling pass). Everything else — the publish cursor and
//! its tests — is private to this module.

use std::sync::Arc;

use futures::channel::mpsc;
use synthia_provider::{BlockAssembler, ContentPart, StreamChunk};

use super::SampleOutcome;
use crate::events::{AgentEvent, SystemEvent};

/// Mutable state owned by [`StreamSink`] during a single LLM sampling
/// pass. Wrapped in `Arc<Mutex<>>` because the streaming provider's
/// callback (`FnMut + Send + 'static`) outlives the borrow scope of
/// `sample_once`.
///
/// `parking_lot::Mutex`, not `std`'s: the guard is taken on every
/// streamed chunk, and a poisoned `std` mutex would turn a panic
/// anywhere under the lock into a panic here too — on a path that has
/// no `SessionEnded` to report it with.
#[derive(Debug, Default, Clone)]
pub(crate) struct ChunkState(Arc<parking_lot::Mutex<ChunkStateInner>>);

#[derive(Debug, Default)]
struct ChunkStateInner {
    /// The provider crate's chunk-to-message fold. Owns the partial
    /// tool buffers, the `IsDone` dedup, the reasoning accumulator,
    /// the usage/stop-reason slots, and the JSON repair applied to
    /// malformed tool arguments.
    assembler: BlockAssembler,
    /// How many of the assembler's parts have already been published
    /// as [`AgentEvent::Model`]. Parts only ever append, so everything
    /// from this cursor on is exactly the set the wire has not seen.
    published: usize,
    /// R29-Phase-L: monotonic ordinal of the next streamed
    /// chunk within this sampling pass. Stamped onto every
    /// emitted `SessionEvent::AssistantChunk` so the durable
    /// log preserves the exact delta order and
    /// `assemble_chunks` can rebuild the text deterministically.
    chunk_seq: u64,
}

/// Typed wrapper over the mpsc sender. Owns an `Arc` to the sender
/// so it can be cloned into `'static` streaming callbacks.
#[derive(Clone)]
pub(crate) struct StreamSink {
    pub(super) tx: Arc<mpsc::UnboundedSender<AgentEvent>>,
    /// R29-Phase-L: optional durable-chunk sink. When present,
    /// every streamed delta is preserved verbatim as
    /// [`synthia_session::SessionEvent::AssistantChunk`] alongside
    /// the assembled `assistant_message` — replay can then
    /// reproduce the exact streaming sequence, not just the
    /// final concatenation.
    pub(super) typed_sink: Option<synthia_session::TypedEventSink>,
}

impl StreamSink {
    /// Translate one streaming chunk into events + state mutation.
    pub(in crate::agent::re_act) fn ingest_chunk(
        &self,
        state: &ChunkState,
        chunk: StreamChunk,
    ) {
        let mut guard = state.0.lock();
        // R29-Phase-L: preserve the raw delta durably BEFORE
        // folding it into the assembled turn. Only emitted when a
        // typed sink is wired, so the in-memory test harness pays
        // nothing.
        if let Some(sink) = &self.typed_sink {
            let (delta, finish_reason) = describe_chunk(&chunk);
            let chunk_seq = guard.chunk_seq;
            guard.chunk_seq += 1;
            sink.record(synthia_session::TypedEventRecord::new(
                synthia_session::SessionEvent::AssistantChunk {
                    seq: 0,
                    ts: String::new(),
                    chunk_seq,
                    finish_reason,
                    data: serde_json::json!({ "delta": delta }),
                },
            ));
        }
        handle_chunk(chunk, &mut guard, &self.tx);
    }

    /// Finish the sampling pass and hand back the assembled turn.
    ///
    /// Safe to call before `IsDone`: the assembler drains any tool
    /// buffer the stream never closed (network cut-off, missing
    /// `ToolCallEnd`), so a partial tool call still reaches both the
    /// wire and the history exactly once.
    pub(in crate::agent::re_act) fn finalize_outcome(
        &self,
        state: &ChunkState,
    ) -> SampleOutcome {
        let mut guard = state.0.lock();
        let (result, parts, _incomplete) = guard.assembler.finalize_assistant();
        // The drain above can append tool calls the stream never
        // closed; publish whatever is new before returning.
        publish_parts(&parts, &mut guard.published, &self.tx);
        SampleOutcome {
            assistant_text: result.text,
            tool_uses: result.tool_calls,
            // The terminal stop reason rides the provider's
            // `CompletionResponse`, which the caller
            // (`sample_once`) holds; the chunk accumulator does
            // not need a second copy of it.
            stop_reason: None,
            parts,
        }
    }

    // -- typed emit helpers ----------------------------------------------

    pub(in crate::agent::re_act) fn emit(&self, event: AgentEvent) {
        let _ = self.tx.unbounded_send(event);
    }

    pub(in crate::agent::re_act) fn system(&self, event: SystemEvent) {
        self.emit(AgentEvent::System(event));
    }

    /// Publish one assembled assistant part. Used by the commit path
    /// for tool results, which are not part of the sampled stream and
    /// so never pass through the cursor above.
    pub(in crate::agent::re_act) fn model(&self, part: ContentPart) {
        self.emit(AgentEvent::Model(part));
    }
}

// ---------------------------------------------------------------------------
// Free helpers — kept because they operate on plain data, no state.
// ---------------------------------------------------------------------------

/// Project one chunk onto the `(delta, finish_reason)` pair the
/// durable `assistant_chunk` event carries.
///
/// Only text content contributes a delta; tool-call traffic is
/// already covered by the `tool_call` / `tool_result` events, and
/// re-serialising it into the chunk log would double-count on
/// replay. A terminal stop reason rides on the chunk that carries
/// it (`Stop` or the final `IsDone`).
fn describe_chunk(chunk: &StreamChunk) -> (String, Option<String>) {
    match chunk {
        StreamChunk::Content(ContentPart::Text(t)) => (t.text.clone(), None),
        StreamChunk::Stop(reason) => (String::new(), Some(reason.clone())),
        StreamChunk::IsDone { result } => {
            (String::new(), result.stop_reason.clone())
        }
        _ => (String::new(), None),
    }
}

/// Publish every part from `published` onward as
/// [`AgentEvent::Model`], advancing the cursor to the end.
///
/// This is the whole reason the module holds a cursor: the assembler
/// tells us what the turn contains, but the wire wants each part
/// exactly once and in wire order. Parts are append-only, so one
/// index is enough — and it is the *same* index whether the part came
/// from a streamed chunk or from the assembler's end-of-stream drain,
/// which is what makes the straggler case (a tool call the stream
/// never closed) publish once rather than twice.
fn publish_parts(
    parts: &[ContentPart],
    published: &mut usize,
    tx: &Arc<mpsc::UnboundedSender<AgentEvent>>,
) {
    while *published < parts.len() {
        let _ = tx.unbounded_send(AgentEvent::Model(parts[*published].clone()));
        *published += 1;
    }
}

/// Fold one chunk into the accumulator, then publish whatever parts
/// it produced.
///
/// The `IsDone` chunk is the only one that also carries terminal
/// traffic (the authoritative usage, then `ModelDone`), and it is
/// emitted *after* its parts so a consumer sees the turn before the
/// summary of it. Intermediate `Usage` chunks are deliberately not
/// surfaced: providers such as Anthropic emit `message_delta.usage`
/// before the terminal `message.usage`, and the latter is the
/// authoritative count, so consumers see exactly one `Usage` per
/// iteration instead of two.
fn handle_chunk(
    chunk: StreamChunk,
    state: &mut ChunkStateInner,
    tx: &Arc<mpsc::UnboundedSender<AgentEvent>>,
) {
    let done = match &chunk {
        StreamChunk::IsDone { result } => Some((**result).clone()),
        _ => None,
    };
    state.assembler.push(chunk);
    {
        // Split the borrow so the slice read and the cursor write do
        // not both go through `state`.
        let ChunkStateInner {
            assembler,
            published,
            ..
        } = state;
        publish_parts(assembler.parts(), published, tx);
    }
    let Some(result) = done else {
        return;
    };
    let usage = &result.usage;
    let _ = tx.unbounded_send(AgentEvent::usage(
        usage.prompt_tokens,
        usage.completion_tokens,
        usage.cache_read_tokens,
        usage.cache_write_tokens,
    ));
    let _ = tx.unbounded_send(AgentEvent::ModelDone(result));
}

#[cfg(test)]
mod tests {
    //! Publish-cursor tests.
    //!
    //! The fold itself is [`synthia_provider::BlockAssembler`]'s
    //! contract and is tested there. What is only true *here* is the
    //! publishing rule: every part reaches the wire exactly once, in
    //! order, no matter which half of the stream produced it. These
    //! tests drive real chunks through [`StreamSink`] so the cursor is
    //! exercised through its production entry points.

    use super::*;

    fn sink() -> (StreamSink, mpsc::UnboundedReceiver<AgentEvent>) {
        let (tx, rx) = mpsc::unbounded();
        (
            StreamSink {
                tx: Arc::new(tx),
                typed_sink: None,
            },
            rx,
        )
    }

    /// Every `AgentEvent::Model` text/reasoning/tool-id published so
    /// far, in order.
    fn model_parts(
        rx: &mut mpsc::UnboundedReceiver<AgentEvent>,
    ) -> Vec<String> {
        let mut seen = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let AgentEvent::Model(part) = event {
                seen.push(match part {
                    ContentPart::Text(t) => format!("text:{}", t.text),
                    ContentPart::Reasoning(r) => format!("reason:{}", r.text),
                    ContentPart::ToolUse(tu) => format!("call:{}", tu.id),
                    other => format!("other:{other:?}"),
                });
            }
        }
        seen
    }

    /// A text delta, a reasoning delta and a completed tool call are
    /// each published exactly once, in the order the provider sent
    /// them — reasoning included, which the in-module fold used to
    /// drop.
    #[test]
    fn publishes_every_part_once_in_wire_order() {
        let (sink, mut rx) = sink();
        let state = ChunkState::default();

        for chunk in [
            StreamChunk::Content(ContentPart::Text(
                synthia_provider::TextContent {
                    text: "hello".to_string(),
                    cache_control: None,
                },
            )),
            StreamChunk::Content(ContentPart::Reasoning(
                synthia_provider::ReasoningContent {
                    text: "thinking".to_string(),
                    signature: Some("sig".to_string()),
                },
            )),
            StreamChunk::ToolCallStart {
                id: "c1".to_string(),
                name: "echo".to_string(),
                arguments: serde_json::json!({}),
            },
            StreamChunk::ToolCallDelta {
                id: "c1".to_string(),
                arguments_delta: "{}".to_string(),
            },
            StreamChunk::ToolCallEnd {
                id: "c1".to_string(),
            },
        ] {
            sink.ingest_chunk(&state, chunk);
        }

        assert_eq!(
            model_parts(&mut rx),
            vec![
                "text:hello".to_string(),
                "reason:thinking".to_string(),
                "call:c1".to_string(),
            ],
            "each part must be published once, in wire order"
        );
    }

    /// A tool call the stream never closed is still published — once —
    /// when the pass is finalized.
    ///
    /// The cursor is what makes this true: the assembler's drain
    /// appends the straggler to the same parts vec the cursor walks,
    /// so the drain cannot double-publish what the stream already
    /// closed, and cannot skip what it did not.
    #[test]
    fn publishes_unclosed_tool_call_once_at_finalize() {
        let (sink, mut rx) = sink();
        let state = ChunkState::default();

        sink.ingest_chunk(
            &state,
            StreamChunk::ToolCallStart {
                id: "partial".to_string(),
                name: "echo".to_string(),
                arguments: serde_json::json!({}),
            },
        );
        sink.ingest_chunk(
            &state,
            StreamChunk::ToolCallDelta {
                id: "partial".to_string(),
                arguments_delta: "{}".to_string(),
            },
        );
        // No `ToolCallEnd`: the stream is cut off mid-call.
        let outcome = sink.finalize_outcome(&state);

        assert_eq!(
            model_parts(&mut rx),
            vec!["call:partial".to_string()],
            "the straggler must reach the wire once"
        );
        assert_eq!(
            outcome
                .tool_uses
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["partial"],
            "and once in the assembled turn"
        );
    }

    /// Calling `finalize_outcome` twice publishes nothing the second
    /// time — the cursor has already passed every part.
    #[test]
    fn finalize_is_idempotent_on_the_wire() {
        let (sink, mut rx) = sink();
        let state = ChunkState::default();

        sink.ingest_chunk(
            &state,
            StreamChunk::Content(ContentPart::Text(
                synthia_provider::TextContent {
                    text: "answer".to_string(),
                    cache_control: None,
                },
            )),
        );
        let first = sink.finalize_outcome(&state);
        let _ = model_parts(&mut rx);

        let second = sink.finalize_outcome(&state);

        assert_eq!(first.assistant_text, "answer");
        assert!(
            model_parts(&mut rx).is_empty(),
            "a second finalize must not republish the turn"
        );
        assert!(
            second.parts.is_empty(),
            "and must not hand back a turn it already handed back"
        );
    }
}
