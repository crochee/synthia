//! [`EventSink`] — the strategy-facing event channel.
//!
//! A thin, cloneable handle over the run's event stream plus the
//! optional typed sink. Clone it into whatever concurrent work the
//! strategy spawns; dropping every clone closes the caller's stream,
//! which is how a run ends.
//!
//! Kept in its own file because the sink is a small concern with a
//! clear single responsibility — every method is a publish-or-query
//! primitive — and merging it into `mod.rs` (or `runtime.rs`) would
//! add a fourth concern to either.

use std::{fmt, sync::Arc};

use futures::channel::mpsc;
use synthia_core::CancelToken;
use synthia_session::{SessionEvent, TypedEventRecord, TypedEventSink};

use crate::events::{AgentEvent, SessionEndReason, SystemEvent};

/// The strategy-facing event channel.
///
/// The channel is `futures::channel::mpsc`, not a runtime's: an
/// out-of-tree [`ReasoningStrategy`](super::ReasoningStrategy) that
/// builds its own sink must not have to depend on tokio to publish
/// events, and the loop that receives them is runtime-neutral too.
#[derive(Clone)]
pub struct EventSink {
    tx: Arc<mpsc::UnboundedSender<AgentEvent>>,
    typed_sink: Option<TypedEventSink>,
}

impl EventSink {
    /// Wrap the run's sender and typed sink.
    ///
    /// The sender is `futures::channel::mpsc::unbounded()`'s — the
    /// same neutral channel
    /// [`Agent::run`](crate::agent::Agent::run) hands back as the
    /// caller's stream. The loop is the production constructor; this is
    /// public so an out-of-tree [`ReasoningStrategy`](super::ReasoningStrategy)
    /// can be exercised without an agent around it.
    #[must_use]
    pub fn new(
        tx: Arc<mpsc::UnboundedSender<AgentEvent>>,
        typed_sink: Option<TypedEventSink>,
    ) -> Self {
        Self { tx, typed_sink }
    }

    /// Publish one event. A closed stream (the caller dropped the
    /// run) makes this a no-op rather than an error: a strategy that
    /// keeps working after the consumer left is wasteful, not broken —
    /// check [`EventSink::is_closed`] when that matters.
    pub fn emit(&self, event: AgentEvent) {
        let _ = self.tx.unbounded_send(event);
    }

    /// Publish one structural event to the typed sink (iteration /
    /// step / usage boundaries). No-op when the deployment did not
    /// install one.
    pub fn emit_typed(&self, event: SessionEvent) {
        if let Some(sink) = &self.typed_sink {
            sink.record(TypedEventRecord::new(event));
        }
    }

    /// The typed sink, when installed.
    #[must_use]
    pub fn typed_sink(&self) -> Option<&TypedEventSink> {
        self.typed_sink.as_ref()
    }

    /// Whether the consumer has dropped the run's stream.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    /// Publish one text delta — the common case, and the shape the
    /// streaming provider's callback produces.
    pub fn text_delta(&self, text: impl Into<String>) {
        self.emit(AgentEvent::text_delta(text));
    }

    /// The raw sender + typed sink. Used by the strategies in this
    /// crate to build the internal streaming sink; not part of the
    /// public contract.
    pub(crate) fn into_parts(
        self,
    ) -> (
        Arc<mpsc::UnboundedSender<AgentEvent>>,
        Option<TypedEventSink>,
    ) {
        (self.tx, self.typed_sink)
    }
}

// ─────────────────────────────────────────────────────────────────────
// Run boundaries
//
// Every strategy opens with the same three lines and closes with one of
// the same three outcomes, so the vocabulary for them lives here rather
// than being spelled out — slightly differently — in each strategy.
// ─────────────────────────────────────────────────────────────────────

impl EventSink {
    /// Publish `SessionStarted`, or end the run immediately as
    /// `Cancelled` when the token already fired.
    ///
    /// Returns `false` when the caller must stop without doing any work:
    /// a pre-cancelled run spends nothing.
    pub fn begin(&self, cancel: &Arc<dyn CancelToken>) -> bool {
        self.emit(AgentEvent::System(SystemEvent::SessionStarted {
            session_id: String::new(),
        }));
        if cancel.is_cancelled() {
            self.emit(AgentEvent::System(SystemEvent::SessionEnded {
                reason: SessionEndReason::Cancelled,
            }));
            return false;
        }
        true
    }

    /// End the run — as `Cancelled` when the token fired during it,
    /// `Completed` otherwise.
    pub fn finish(&self, cancel: &Arc<dyn CancelToken>) {
        let reason = if cancel.is_cancelled() {
            SessionEndReason::Cancelled
        } else {
            SessionEndReason::Completed
        };
        self.emit(AgentEvent::System(SystemEvent::SessionEnded { reason }));
    }

    /// End the run as a fatal `Error` carrying `message`.
    ///
    /// Strategies signal failure this way rather than by returning `Err`
    /// — the event stream is the contract, so a consumer that only reads
    /// the stream still learns why the run stopped.
    pub fn fail(&self, message: String) {
        self.emit(AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Error(message),
        }));
    }
}

impl fmt::Debug for EventSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventSink")
            .field("closed", &self.is_closed())
            .field("typed", &self.typed_sink.is_some())
            .finish()
    }
}
