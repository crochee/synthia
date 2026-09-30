//! [`AgentHandle`] — runtime-neutral wrapper around an
//! [`Agent::run`] call.
//!
//! ## Two run modes
//!
//! ### Blocking — `AgentHandle::run`
//!
//! Returns the agent's event stream directly. The caller drives
//! the stream to completion; cancellation is observed by
//! `Arc<dyn CancelToken>::cancel()`. This is the existing
//! behaviour and matches every consumer that already uses
//! `Agent::run`.
//!
//! ### Detached — `AgentHandle::spawn_detached`
//!
//! Prepares a background handle around the run and returns a
//! [`DetachedAgent`] immediately. The run itself starts on the first
//! [`DetachedAgent::join`] (this type holds no executor, so it cannot
//! detach a task of its own); from then on the handle exposes:
//!
//! - `join()` — drive the run to completion; yields the final
//!   [`AgentEvent`] (typically `SessionEnded`). Idempotent: the
//!   terminal event is memoized, so calling it twice does not run the
//!   agent twice.
//! - `try_event()` — non-blocking receive of the next buffered event,
//!   without awaiting. `Ok(None)` means "nothing yet"; once the run has
//!   finished and its buffer has drained it returns [`DetachedClosed`],
//!   so a polling loop always terminates.
//! - `cancel()` / `cancel_token()` — explicitly abort the run. This is
//!   the pi-subagents `cancel-the-wait-not-the-work` split: dropping
//!   the handle stops the caller waiting, `cancel()` stops the work.
//!
//! ## Runtime neutrality
//!
//! `AgentHandle::spawn_detached` is built on
//! `futures::channel::mpsc` + `futures::stream` — both
//! runtime-neutral. The handle does not require tokio.

use std::{pin::Pin, sync::Arc};

use futures::{Stream, lock::Mutex as AsyncMutex};
use synthia_core::CancelToken;
use tracing::warn;

use crate::{
    agent::{Agent, AgentEvent, AgentInput},
    events::SystemEvent,
};

/// The run's event stream, held for the lifetime of a
/// [`DetachedAgent`] so a repeated `join` resumes the same run rather
/// than starting a new one.
type AgentEventStream = Pin<Box<dyn Stream<Item = AgentEvent> + Send>>;

/// Lightweight wrapper around any [`Agent`] that gives the caller
/// a uniform surface for either blocking on a run or deferring it to a
/// [`DetachedAgent`].

#[derive(Clone)]
pub struct AgentHandle {
    agent: Arc<dyn Agent>,
}

impl std::fmt::Debug for AgentHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentHandle")
            .field("agent", &self.agent.descriptor().name)
            .finish()
    }
}

impl AgentHandle {
    /// Wrap a single agent into a handle.
    pub fn new(agent: Arc<dyn Agent>) -> Self {
        Self { agent }
    }

    /// The underlying agent descriptor (for diagnostics).
    pub fn agent(&self) -> &dyn Agent {
        self.agent.as_ref()
    }

    /// Run the agent **blocking** — returns the agent's event
    /// stream directly.
    pub async fn run(
        &self,
        input: AgentInput,
        cancel: Arc<dyn CancelToken>,
    ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>> {
        self.agent.run(input, cancel).await
    }

    /// Run the agent **detached** — prepares a background handle the
    /// caller drives with [`DetachedAgent::join`] and polls with
    /// [`DetachedAgent::try_event`].
    ///
    /// Nothing runs yet: this type holds no executor, so the agent is
    /// started by the first [`DetachedAgent::join`]. Until then
    /// [`DetachedAgent::try_event`] reports `Ok(None)` — the buffer is
    /// open, just empty.
    pub fn spawn_detached(
        &self,
        input: AgentInput,
        cancel: Arc<dyn CancelToken>,
    ) -> DetachedAgent {
        self.spawn_detached_with_capacity(input, cancel, 1024)
    }

    /// Same as [`spawn_detached`](Self::spawn_detached) but lets
    /// the caller pick the event-channel capacity.
    ///
    /// `buffer_capacity` is the *requested* size; the channel is
    /// **bounded and lossy**. A run that outruns the poller drops
    /// events once the buffer is full, and the losses are counted and
    /// reported **once**, with the total, when the run ends. The bound
    /// is what keeps a caller who stopped polling from growing memory
    /// without limit — [`DetachedAgent::join`] still returns the
    /// terminal event regardless, so only the polled transcript can be
    /// truncated. A caller who needs every event should drive
    /// [`Agent::run`](crate::agent::Agent::run)'s stream directly.
    pub fn spawn_detached_with_capacity(
        &self,
        input: AgentInput,
        cancel: Arc<dyn CancelToken>,
        buffer_capacity: usize,
    ) -> DetachedAgent {
        let (tx, rx) = futures::channel::mpsc::channel(buffer_capacity);
        DetachedAgent {
            agent: Arc::clone(&self.agent),
            input,
            cancel,
            rx: parking_lot::Mutex::new(rx),
            capacity: buffer_capacity,
            slot: AsyncMutex::new(RunSlot::Idle(tx)),
            descriptor_name: self.agent.descriptor().name.clone(),
        }
    }
}

/// The whole lifecycle of a detached run, behind one lock.
///
/// One enum rather than a stream slot plus a memo plus a state flag:
/// the three facts are correlated ("finished" implies "the stream is
/// spent"), and holding them under separate locks lets a queued caller
/// observe a combination that never existed — an exhausted stream with
/// an unset memo, which is exactly what makes a second `join` report
/// `EmptyStream` for a run that succeeded. Here that state is not
/// representable.
enum RunSlot {
    /// Not started: the first `join` calls `Agent::run`. Holds the
    /// channel's sender so the buffer stays open (and `try_event`
    /// reports "nothing yet" rather than "closed") until the run is
    /// driven.
    Idle(Sender),
    /// In flight. The stream is *retained* after its terminal event so
    /// the handle can move to `Done` rather than restarting the agent.
    ///
    /// The sender lives here, not on the handle: dropping it is what
    /// tells a `try_event` caller the run is over. Keeping it on the
    /// handle would leave the channel open forever, so the "closed"
    /// signal could never fire.
    Running {
        stream: AgentEventStream,
        tx: Sender,
    },
    /// Finished; the terminal event is kept for replay. Boxed because
    /// an `AgentEvent` is ~300 bytes and this variant is only ever
    /// read, never mutated in place. The sender is gone — the buffer
    /// drains and then reports closed.
    Done(Box<AgentEvent>),
    /// The stream ended without a terminal event. Sender gone, same as
    /// `Done`.
    Exhausted,
}

/// The run's event sender, kept beside the stream it feeds.
type Sender = futures::channel::mpsc::Sender<AgentEvent>;

/// A handle returned by [`AgentHandle::spawn_detached`].
pub struct DetachedAgent {
    agent: Arc<dyn Agent>,
    input: AgentInput,
    cancel: Arc<dyn CancelToken>,
    rx: parking_lot::Mutex<futures::channel::mpsc::Receiver<AgentEvent>>,
    /// Bound on `rx`, kept so the overflow warning can name the number
    /// the operator would have to raise.
    capacity: usize,
    /// The run's lifecycle. Async-aware because the guard is held while
    /// the stream is driven, and the run itself is awaited while it is
    /// first created; that hold is what serializes concurrent joins.
    slot: AsyncMutex<RunSlot>,
    descriptor_name: String,
}

impl std::fmt::Debug for DetachedAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = match self.slot.try_lock() {
            Some(guard) => match &*guard {
                RunSlot::Idle(_) => "idle",
                RunSlot::Running { .. } => "running",
                RunSlot::Done(_) => "completed",
                RunSlot::Exhausted => "exhausted",
            },
            None => "busy",
        };
        f.debug_struct("DetachedAgent")
            .field("agent", &self.descriptor_name)
            .field("state", &state)
            .finish()
    }
}

impl DetachedAgent {
    /// Cancel the underlying agent run.
    pub async fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Look up the cancel token the caller can fire directly.
    pub fn cancel_token(&self) -> Arc<dyn CancelToken> {
        Arc::clone(&self.cancel)
    }

    /// Try-receive the next buffered event without awaiting.
    ///
    /// The run starts at the first [`Self::join`], so before that this
    /// reports `Ok(None)` — "nothing yet". Once the run has finished
    /// *and* its buffered tail has been drained, the sender is gone and
    /// this reports [`DetachedClosed`]: `Ok(None)` never means "no more
    /// events will ever arrive", `Err` does.
    ///
    /// The buffer is bounded and lossy (see
    /// [`AgentHandle::spawn_detached_with_capacity`]): a run that
    /// outruns the poller loses events, and the total is reported once
    /// when the run ends. The terminal event is not affected —
    /// [`Self::join`] returns it whether or not it fit in the buffer.
    pub fn try_event(&self) -> Result<Option<AgentEvent>, DetachedClosed> {
        match self.rx.lock().try_recv() {
            Ok(event) => Ok(Some(event)),
            // `futures::channel::mpsc` reports `Closed` only once the
            // queue is empty *and* every sender is dropped, so a
            // finished run still yields its buffered events first.
            Err(futures::channel::mpsc::TryRecvError::Empty) => Ok(None),
            Err(futures::channel::mpsc::TryRecvError::Closed) => {
                Err(DetachedClosed)
            }
        }
    }

    /// Drive the agent run to completion. Returns the **last**
    /// event emitted (typically `SessionEnded`).
    ///
    /// The run starts here, not at
    /// [`AgentHandle::spawn_detached`]: this type holds no executor, so
    /// it cannot detach a task of its own.
    ///
    /// Joining is idempotent, including from several tasks at once:
    /// the agent runs exactly once (a run's tool side effects are not
    /// repeatable), every caller that arrives after it finishes gets
    /// the same terminal event, and the whole lifecycle sits behind one
    /// lock, so no caller can observe a finished run as an exhausted
    /// stream.
    pub async fn join(&self) -> Result<AgentEvent, DetachedError> {
        // Held for the whole drive: serializes concurrent joins, keeps
        // the stream alive across them, and makes the transition into
        // `Done` atomic with respect to any queued caller.
        let mut slot = self.slot.lock().await;

        if matches!(*slot, RunSlot::Idle(_)) {
            // Await the run **before** touching the slot. A caller that
            // drops this future mid-await must leave the slot exactly
            // as it found it: installing a placeholder first (to move
            // the sender out) would leak that placeholder as a terminal
            // state, and the next `join` would report `EmptyStream` for
            // a run that never started. `Agent::run` is free to suspend
            // before yielding its stream, so that is reachable.
            let stream = self
                .agent
                .run(self.input.clone(), Arc::clone(&self.cancel))
                .await;
            // No await between these two statements. The lock is held,
            // so this window is synchronous and nothing can observe the
            // intermediate placeholder.
            let RunSlot::Idle(tx) =
                std::mem::replace(&mut *slot, RunSlot::Exhausted)
            else {
                unreachable!("matched Idle above")
            };
            *slot = RunSlot::Running { stream, tx };
        }

        let (stream, tx) = match &mut *slot {
            RunSlot::Running { stream, tx } => (stream, tx),
            // Already finished: replay rather than re-run.
            RunSlot::Done(event) => return Ok((**event).clone()),
            RunSlot::Exhausted => return Err(DetachedError::EmptyStream),
            RunSlot::Idle(_) => unreachable!("started above"),
        };

        let mut last: Option<AgentEvent> = None;
        let mut dropped: u64 = 0;
        use futures::StreamExt;
        while let Some(event) = stream.as_mut().next().await {
            // The buffer is bounded, so a poller that falls behind
            // loses events. Counted rather than ignored: silent loss
            // would make a truncated transcript look complete.
            if tx.try_send(event.clone()).is_err() {
                dropped += 1;
            }
            let is_terminal = matches!(
                event,
                AgentEvent::System(SystemEvent::SessionEnded { .. })
            );
            last = Some(event);
            if is_terminal {
                break;
            }
        }

        if dropped > 0 {
            warn!(
                dropped,
                requested_capacity = self.capacity,
                "detached run overflowed its event buffer; those events \
                 were dropped from the polled transcript. Poll \
                 `try_event` while the run is in flight, or request a \
                 larger capacity via `spawn_detached_with_capacity`. \
                 The terminal event is unaffected."
            );
        }

        match last {
            Some(event) => {
                // Published while the lock is still held: the memo and
                // the drive are one critical section, so a queued join
                // cannot slip in between them. The assignment also
                // drops the sender, closing the buffer behind us.
                *slot = RunSlot::Done(Box::new(event.clone()));
                Ok(event)
            }
            // No terminal event: the run is recorded as exhausted, so a
            // retry reports the same thing instead of silently
            // reporting success.
            None => {
                *slot = RunSlot::Exhausted;
                Err(DetachedError::EmptyStream)
            }
        }
    }
}

/// Error returned by [`DetachedAgent::try_event`]: the run has
/// finished and its buffered events have all been drained, so no
/// further event will ever arrive.
#[derive(Debug, thiserror::Error)]
#[error("detached agent handle has been closed")]
pub struct DetachedClosed;

/// Error returned by [`DetachedAgent::join`].
#[derive(Debug, thiserror::Error)]
pub enum DetachedError {
    /// The agent stream completed without yielding any events.
    #[error("agent stream produced no events")]
    EmptyStream,
}
