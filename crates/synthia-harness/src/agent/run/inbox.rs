//! [`RunInbox`] — interactive message injection for a running
//! agent (pi `getSteeringMessages` / `getFollowUpMessages`
//! parity).
//!
//! A run inbox is the seam between an interactive front end and
//! an in-flight [`ReActLoop`](super::re_act) session:
//!
//! - **Steering** messages are typed while the run is in flight.
//!   The loop drains them at the start of every iteration (and
//!   again after long prepare steps such as compaction) and
//!   injects them as trailing user messages before the next
//!   assistant response, so the model sees the user's course
//!   correction without interrupting the run.
//! - **Follow-up** messages are polled when the run is about to
//!   stop (the model produced a final answer). A non-empty poll
//!   appends the messages and continues the loop instead of
//!   stopping — the "one more thing" revival.
//!
//! Both methods drain-and-return: an implementation hands over
//! everything it has queued and resets to empty. The default
//! trait implementations return empty vectors, so an
//! implementation only overrides the half it cares about.
//!
//! The trait is runtime-neutral (no tokio types); the built-in
//! [`MpscInbox`] is backed by [`futures::channel::mpsc`].

use async_trait::async_trait;
use futures::channel::mpsc;
use parking_lot::Mutex;
use synthia_provider::Message;

/// Async, runtime-neutral source of interactive messages for a
/// running agent session.
///
/// Implementations must be cheap to poll: the loop calls
/// [`RunInbox::take_steering`] once per iteration and
/// [`RunInbox::take_follow_up`] only at would-be stops. Both
/// methods drain everything currently queued; an empty return is
/// the "nothing to inject" signal.
#[async_trait]
pub trait RunInbox: Send + Sync {
    /// Take every steering message queued for the in-flight run.
    ///
    /// Steering messages are injected as trailing user messages
    /// before the next assistant response. The default returns
    /// an empty vector (no steering).
    async fn take_steering(&self) -> Vec<Message> {
        Vec::new()
    }

    /// Take every follow-up message waiting to revive a stopping
    /// run.
    ///
    /// Polled exactly when the loop is about to stop on a final
    /// answer; a non-empty return appends the messages and
    /// continues the run. The default returns an empty vector
    /// (runs stop as usual).
    async fn take_follow_up(&self) -> Vec<Message> {
        Vec::new()
    }
}

/// Producer half of [`MpscInbox`]. Clone it into every UI
/// component that feeds the run; each clone pushes into the same
/// queues the agent drains.
#[derive(Clone)]
pub struct RunInboxHandle {
    steering: mpsc::UnboundedSender<Message>,
    follow_up: mpsc::UnboundedSender<Message>,
}

impl RunInboxHandle {
    /// Queue a steering message for the in-flight run.
    ///
    /// Fails only when the owning run (and its [`MpscInbox`]) has
    /// been dropped — e.g. the session ended and the front end
    /// has not noticed yet.
    pub fn send_steering(
        &self,
        message: Message,
    ) -> Result<(), Box<mpsc::TrySendError<Message>>> {
        self.steering.unbounded_send(message).map_err(Box::new)
    }

    /// Queue a follow-up message that revives the run when it is
    /// about to stop.
    ///
    /// Fails only when the owning run (and its [`MpscInbox`]) has
    /// been dropped.
    pub fn send_follow_up(
        &self,
        message: Message,
    ) -> Result<(), Box<mpsc::TrySendError<Message>>> {
        self.follow_up.unbounded_send(message).map_err(Box::new)
    }
}

/// Built-in [`RunInbox`] over two unbounded
/// [`futures::channel::mpsc`] channels (steering + follow-up).
///
/// Created via [`MpscInbox::channel`], which returns the inbox
/// alongside the [`RunInboxHandle`] used to feed it. Draining is
/// non-blocking: each `take_*` empties the queue in FIFO order
/// and a subsequent call returns nothing until more is sent.
///
/// Unbounded on purpose: the agent loop is the sole consumer and
/// drains every iteration, and a UI producer must never block
/// (or await) to hand the agent a message.
pub struct MpscInbox {
    steering: Mutex<mpsc::UnboundedReceiver<Message>>,
    follow_up: Mutex<mpsc::UnboundedReceiver<Message>>,
}

impl MpscInbox {
    /// Create an inbox and its producer handle.
    pub fn channel() -> (Self, RunInboxHandle) {
        let (steering_tx, steering_rx) = mpsc::unbounded();
        let (follow_up_tx, follow_up_rx) = mpsc::unbounded();
        (
            Self {
                steering: Mutex::new(steering_rx),
                follow_up: Mutex::new(follow_up_rx),
            },
            RunInboxHandle {
                steering: steering_tx,
                follow_up: follow_up_tx,
            },
        )
    }

    /// Drain a receiver in FIFO order. `Err(_)` — empty queue or
    /// a dropped producer — ends the drain; a dropped producer
    /// with buffered messages still yields them first.
    fn drain(
        receiver: &Mutex<mpsc::UnboundedReceiver<Message>>,
    ) -> Vec<Message> {
        let mut guard = receiver.lock();
        let mut out = Vec::new();
        while let Ok(message) = guard.try_recv() {
            out.push(message);
        }
        out
    }
}

#[async_trait]
impl RunInbox for MpscInbox {
    async fn take_steering(&self) -> Vec<Message> {
        Self::drain(&self.steering)
    }

    async fn take_follow_up(&self) -> Vec<Message> {
        Self::drain(&self.follow_up)
    }
}
