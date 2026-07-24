//! The [`AgentEventEmitter`] unbounded-MPSC emitter.
//!
//! Wraps the sender side of a
//! `tokio::sync::mpsc::unbounded_channel` and provides a
//! simple `emit()` method. Call
//! [`AgentEventEmitter::pair`] to obtain a matched
//! sender/receiver pair.

use tokio::sync::mpsc;

use super::event_enum::AgentEvent;

/// Unbounded MPSC emitter for sending [`AgentEvent`]s
/// from within async contexts.
///
/// Wraps the sender side of a `tokio::sync::mpsc::unbounded_channel` and
/// provides a simple `emit()` method. Call [`AgentEventEmitter::pair()`]
/// to obtain a matched sender/receiver pair.
pub struct AgentEventEmitter {
    pub(crate) tx: mpsc::UnboundedSender<AgentEvent>,
}

impl AgentEventEmitter {
    /// Create a new emitter / receiver pair.
    ///
    /// Returns `(emitter, receiver)` where `emitter` is an `AgentEventEmitter`
    /// and `receiver` is an `mpsc::UnboundedReceiver<AgentEvent>`.
    pub fn pair() -> (Self, mpsc::UnboundedReceiver<AgentEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    /// Emit an event into the channel. Returns `true` if the event was sent,
    /// `false` if the receiver has been dropped.
    pub fn emit(&self, event: AgentEvent) -> bool {
        self.tx.send(event).is_ok()
    }

    /// Return a reference to the inner sender for cases where direct access
    /// is needed (e.g. cloning for capture in closures).
    pub fn sender(&self) -> &mpsc::UnboundedSender<AgentEvent> {
        &self.tx
    }
}

impl Clone for AgentEventEmitter {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}
