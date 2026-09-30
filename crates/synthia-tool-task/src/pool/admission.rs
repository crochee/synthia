//! Admission lifecycle types: the ticket a running child
//! holds, the outcome of an admission attempt, and the token
//! handed to the waiter a release admits.
//!
//! Ticket and token fields are `pub(super)` so only the pool
//! state machine (in `super::state`) can mint them.

use super::slot::Slot;

/// Handle for one admitted child.
///
/// Tickets are unique across every pool, so releasing one against the
/// wrong pool — or twice — frees nothing instead of corrupting the
/// counts.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct AdmissionTicket {
    pub(super) pool: u64,
    pub(super) seq: u64,
    pub(super) slot: Slot,
}
impl AdmissionTicket {
    /// The ticket's lane.
    #[must_use]
    pub fn slot(self) -> Slot {
        self.slot
    }

    /// Position of this admission in its pool's monotonic sequence.
    /// Queue entries and [`QueuedToken`]s carry the same value, so a
    /// driver can match a waiting spawn to the token that admits it.
    #[must_use]
    pub fn seq(self) -> u64 {
        self.seq
    }
}

/// Outcome of `SubagentPool::admit`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Admission {
    /// There was room: the child may start now. Keep the ticket and
    /// pass it back to `SubagentPool::release` when the child
    /// settles.
    Run(AdmissionTicket),
    /// The lane is saturated: the child waits in its lane's FIFO
    /// queue and will be admitted by the release that frees a slot.
    ///
    /// `position` counts the same-lane children already waiting
    /// (`0` = next to start), and `seq` is the queue entry's handle.
    Queued {
        /// Handle matching this entry's later [`QueuedToken`].
        seq: u64,
        /// Same-lane children ahead of this one.
        position: usize,
    },
}

/// A queued child that a `SubagentPool::release` just admitted.
///
/// The slot is held already: start the child now, and release
/// [`QueuedToken::ticket`] when it settles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueuedToken {
    pub(super) ticket: AdmissionTicket,
    pub(super) seq: u64,
}

impl QueuedToken {
    /// The lane the child was waiting on.
    #[must_use]
    pub fn slot(self) -> Slot {
        self.ticket.slot
    }

    /// The queue entry's handle — the `seq` its
    /// [`Admission::Queued`] reported.
    #[must_use]
    pub fn seq(self) -> u64 {
        self.seq
    }

    /// The admission to release when this child settles.
    #[must_use]
    pub fn ticket(self) -> AdmissionTicket {
        self.ticket
    }
}
