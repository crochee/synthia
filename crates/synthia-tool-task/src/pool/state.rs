//! The two-lane admission state machine.
//!
//! Owns the limits, the per-lane FIFO queues, the held
//! tickets, and the invocation tombstones. See the module
//! docs on [`super`] for the lanes and the lifecycle.

use std::{
    collections::VecDeque,
    sync::atomic::{AtomicU64, Ordering},
};

use super::{
    admission::{Admission, AdmissionTicket, QueuedToken},
    record::{InvocationRecord, MAX_TOMBSTONES},
    slot::{DEFAULT_BACKGROUND_CONCURRENCY, Slot},
};

/// Process-wide pool discriminator so a ticket can never be honoured
/// by a pool it did not come from.
static NEXT_POOL_ID: AtomicU64 = AtomicU64::new(1);

/// A [`SubagentPool`] shared between a parent loop and the children
/// it spawns, behind the crate's usual interior mutability.
pub type SharedSubagentPool = std::sync::Arc<parking_lot::Mutex<SubagentPool>>;

/// Snapshot of every pool counter.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PoolCounts {
    /// Live background children.
    pub background_active: usize,
    /// Live foreground children.
    pub foreground_active: usize,
    /// Live uncharged children (nested spawns).
    pub uncharged_active: usize,
    /// Children waiting for a background slot.
    pub background_queued: usize,
    /// Children waiting for a foreground slot.
    pub foreground_queued: usize,
    /// Finished children still addressable by id.
    pub tombstones: usize,
}

impl PoolCounts {
    /// Total live children across every lane.
    #[must_use]
    pub fn active(&self) -> usize {
        self.background_active + self.foreground_active + self.uncharged_active
    }

    /// Total children waiting for a slot.
    #[must_use]
    pub fn queued(&self) -> usize {
        self.background_queued + self.foreground_queued
    }
}

/// Two-lane subagent admission state machine.
///
/// See the module docs for the lanes and the lifecycle. Construct with
/// [`SubagentPool::new`] and share through [`SharedSubagentPool`].
#[derive(Debug)]
pub struct SubagentPool {
    id: u64,
    background_limit: usize,
    foreground_limit: Option<usize>,
    /// Per-slot FIFO of queue entry sequences, indexed by
    /// [`Slot::index`]. The uncharged slot's queue is never used: an
    /// uncharged child is always admitted.
    queues: [VecDeque<u64>; 3],
    /// Every admitted child that has not been released, in admission
    /// order. The single source of truth for live counts: a lane's
    /// usage is the number of held tickets in it.
    held: Vec<AdmissionTicket>,
    next_seq: u64,
    /// Finished children, oldest first — oldest evicted past
    /// [`MAX_TOMBSTONES`].
    tombstones: VecDeque<(String, InvocationRecord)>,
}

impl Default for SubagentPool {
    fn default() -> Self {
        Self::new()
    }
}

impl SubagentPool {
    /// A pool with the default limits: background bounded at
    /// [`DEFAULT_BACKGROUND_CONCURRENCY`], foreground unbounded.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: NEXT_POOL_ID.fetch_add(1, Ordering::Relaxed),
            background_limit: DEFAULT_BACKGROUND_CONCURRENCY,
            foreground_limit: None,
            queues: [VecDeque::new(), VecDeque::new(), VecDeque::new()],
            held: Vec::new(),
            next_seq: 0,
            tombstones: VecDeque::new(),
        }
    }

    /// Set the background ceiling. `0` is clamped to `1`: a lane that
    /// can never admit anything is a misconfiguration, not a policy.
    #[must_use]
    pub fn with_background_limit(mut self, limit: usize) -> Self {
        self.background_limit = limit.max(1);
        self
    }

    /// Bound the foreground lane.
    ///
    /// `0` means **unlimited** — the default, and pi-subagents'
    /// convention for `maxConcurrentForeground` — so a config ported
    /// from pi cannot accidentally freeze inline delegation. Any
    /// positive value queues the excess instead of exceeding the cap.
    #[must_use]
    pub fn with_foreground_limit(mut self, limit: usize) -> Self {
        self.foreground_limit = (limit > 0).then_some(limit);
        self
    }

    /// The background ceiling.
    #[must_use]
    pub fn background_limit(&self) -> usize {
        self.background_limit
    }

    /// The foreground ceiling, or `None` when unbounded.
    #[must_use]
    pub fn foreground_limit(&self) -> Option<usize> {
        self.foreground_limit
    }

    /// Admit a child, queueing it when its lane is saturated.
    ///
    /// Nested (`Slot::Uncharged`) children are always admitted: they
    /// hold no slot and can never be gated.
    pub fn admit(&mut self, slot: Slot) -> Admission {
        if self.has_room(slot) {
            return Admission::Run(self.take_ticket(slot));
        }
        let position = self.queues[slot.index()].len();
        let seq = self.next_seq();
        self.queues[slot.index()].push_back(seq);
        Admission::Queued { seq, position }
    }

    /// Admit a child only if its lane has room **now**; never queues.
    ///
    /// For a caller that is itself blocking — the `task` tool seam —
    /// queueing would park a child nobody can start, so saturation is
    /// reported instead of buffered.
    pub fn try_admit(&mut self, slot: Slot) -> Option<AdmissionTicket> {
        if !self.has_room(slot) {
            return None;
        }
        Some(self.take_ticket(slot))
    }

    /// Release an admitted child, handing its freed slot to the
    /// earliest waiter **of the same lane** (pi-subagents drains each
    /// pool from the head of its own queue, so a saturated lane never
    /// stalls another lane's queue).
    ///
    /// Returns the token for the newly admitted child, or `None` when
    /// nothing was waiting — or when `ticket` came from another pool,
    /// was never admitted here, or was already released. A stale
    /// release frees nothing rather than corrupting the counts.
    pub fn release(&mut self, ticket: AdmissionTicket) -> Option<QueuedToken> {
        if ticket.pool != self.id {
            return None;
        }
        let index = self.held.iter().position(|held| *held == ticket)?;
        self.held.swap_remove(index);
        // The child held no slot (unbounded or uncharged lane):
        // there is nothing to hand on.
        self.limit(ticket.slot)?;
        let seq = self.queues[ticket.slot.index()].pop_front()?;
        let admitted = AdmissionTicket {
            pool: self.id,
            seq,
            slot: ticket.slot,
        };
        self.held.push(admitted);
        Some(QueuedToken {
            ticket: admitted,
            seq,
        })
    }

    /// The lane of the oldest child still waiting for a slot, or
    /// `None` when nothing is queued.
    #[must_use]
    pub fn next_queued(&self) -> Option<Slot> {
        [Slot::Background, Slot::Foreground]
            .into_iter()
            .filter_map(|slot| {
                self.queues[slot.index()].front().map(|seq| (*seq, slot))
            })
            .min_by_key(|(seq, _)| *seq)
            .map(|(_, slot)| slot)
    }

    /// Children waiting for a slot in `slot`.
    #[must_use]
    pub fn queued(&self, slot: Slot) -> usize {
        self.queues[slot.index()].len()
    }

    /// Children waiting for a slot across every lane.
    #[must_use]
    pub fn queued_len(&self) -> usize {
        self.queued(Slot::Background) + self.queued(Slot::Foreground)
    }

    /// Live children admitted to `slot` and not yet released.
    ///
    /// For a **bounded** lane this is also its slot usage, since every
    /// child admitted to a bounded lane holds a slot. An unbounded
    /// lane has no slots, so there it is a plain live count.
    #[must_use]
    pub fn active(&self, slot: Slot) -> usize {
        self.held
            .iter()
            .filter(|ticket| ticket.slot == slot)
            .count()
    }

    /// Every counter at once.
    #[must_use]
    pub fn counts(&self) -> PoolCounts {
        PoolCounts {
            background_active: self.active(Slot::Background),
            foreground_active: self.active(Slot::Foreground),
            uncharged_active: self.active(Slot::Uncharged),
            background_queued: self.queued(Slot::Background),
            foreground_queued: self.queued(Slot::Foreground),
            tombstones: self.tombstones.len(),
        }
    }

    /// Whether no child is running and nothing is waiting.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.held.is_empty() && self.queued_len() == 0
    }

    /// Remember a finished child so it stays addressable by `id`.
    ///
    /// Callers record completions in settle order; the oldest entry is
    /// evicted once [`MAX_TOMBSTONES`] are stored.
    pub fn record_completion(
        &mut self,
        id: impl Into<String>,
        record: InvocationRecord,
    ) {
        self.tombstones.push_back((id.into(), record));
        while self.tombstones.len() > MAX_TOMBSTONES {
            self.tombstones.pop_front();
        }
    }

    /// The record for a finished child, when it is still addressable.
    /// Newest first: a repeated id resolves to the latest completion.
    #[must_use]
    pub fn lookup(&self, id: &str) -> Option<&InvocationRecord> {
        self.tombstones
            .iter()
            .rev()
            .find(|(entry_id, _)| entry_id == id)
            .map(|(_, record)| record)
    }

    /// Addressable finished children, newest first (pi-subagents
    /// `listTombstones`).
    pub fn tombstones(
        &self,
    ) -> impl Iterator<Item = (&str, &InvocationRecord)> + '_ {
        self.tombstones
            .iter()
            .rev()
            .map(|(id, record)| (id.as_str(), record))
    }

    /// How many finished children are still addressable.
    #[must_use]
    pub fn tombstone_count(&self) -> usize {
        self.tombstones.len()
    }

    // -- internals ----------------------------------------------------

    /// The ceiling of `slot`, or `None` when the lane admits
    /// unconditionally.
    fn limit(&self, slot: Slot) -> Option<usize> {
        match slot {
            Slot::Background => Some(self.background_limit),
            Slot::Foreground => self.foreground_limit,
            // Nested children are subject to no bound: their parent
            // already holds the turn (and possibly a slot).
            Slot::Uncharged => None,
        }
    }

    fn has_room(&self, slot: Slot) -> bool {
        match self.limit(slot) {
            Some(limit) => self.active(slot) < limit,
            None => true,
        }
    }

    /// Charge `slot` to a new child and mint its ticket. The caller
    /// must have checked [`Self::has_room`].
    fn take_ticket(&mut self, slot: Slot) -> AdmissionTicket {
        let ticket = AdmissionTicket {
            pool: self.id,
            seq: self.next_seq(),
            slot,
        };
        self.held.push(ticket);
        ticket
    }

    fn next_seq(&mut self) -> u64 {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        seq
    }
}
