//! In-memory [`EventBus`](crate::EventBus) implementation backed by a
//! bounded ring buffer (PR-1.3).
//!
//! Implementation choices (see `design.md` §D1):
//!
//! - **Storage**: `parking_lot::Mutex<VecDeque<StoredEvent>>` (cap default
//!   1024, push-O(1), pop-front-O(1)) — sufficient for the default
//!   in-memory sink and matching opencode's `event.ts` per-process model.
//! - **Per-source sequencing**: `parking_lot::Mutex<HashMap<EventSource,
//!   SourceSequence>>` — driven by [`crate::event::SourceSequence`]. Each
//!   `emit` allocates one sequence value per source.
//! - **Drop cleanup**: a `Drop` impl emits a `tracing::trace!` event and
//!   flushes any retained bytes so metrics counters can record the
//!   bus lifetime. Persistent storage is the durable sink's concern
//!   (PR-1.4), so the in-memory sink does not spawn a background
//!   `CleanupTask` — `CleanupTask` lands in PR-1.5.
//! - **Eviction policy**: when `len == cap`, the oldest event is dropped
//!   (FIFO). Spec note from `specs/event-v2-system/spec.md` Scenario
//!   "default in-memory sink" expects "bounded ring" semantics; the
//!   closest behavior in opencode is the durable cleanup task — here we
//!   replicate it in-process via FIFO eviction so the sink never blocks.
//!
//! PR-1.3 carries the bounded ring + sequence allocator + Drop impl; PR-1.4
//! adds the `Sqlite` sink behind a feature flag.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use parking_lot::Mutex;
use serde::Serialize;

use crate::{
    EmitOutcome,
    EventBus,
    EventBusError,
    EventSinkKind,
    bridge::EventBusBridge,
    event::{
        EventEnvelope,
        EventMeta,
        EventSource,
        EventVersion,
        PrefixHash,
        SourceSequence,
    },
};

/// Default ring capacity (1024 entries, per spec).
pub const DEFAULT_IN_MEMORY_CAP: usize = 1024;

/// Stored event entry: a versioned envelope carried in the ring.
///
/// Keeping the envelope by value (rather than `Box<dyn Any>`) lets the
/// in-memory sink serve `aggregate_events::<T>()` readers in PR-1.5
/// without an extra downcast hop.
#[derive(Debug, Clone)]
struct StoredEvent {
    /// Monotonic source sequence assigned at emit time.
    version: EventVersion,
    /// 32-byte prefix hash for the source prefix snapshot at emit.
    prefix_hash: PrefixHash,
    /// Wall-clock creation timestamp (ms since Unix epoch).
    created_at_ms: i64,
    /// Raw JSON payload produced by `EventBus::emit`.
    payload: serde_json::Value,
}

/// In-memory bounded ring event bus (PR-1.3, PR-1.5 bridge wiring).
pub struct InMemoryEventBus {
    /// Logical capacity advertised to consumers; coerced to `>= 1`.
    cap: usize,
    /// Bounded ring buffer.
    ring: Mutex<VecDeque<StoredEvent>>,
    /// Per-source monotonic sequence allocator.
    sequences: Mutex<HashMap<EventSource, SourceSequence>>,
    /// Set to `true` once `Drop` has fired. Tests may observe this via
    /// `Self::is_closed`.
    closed: Mutex<bool>,
    /// Optional downstream bridge attached via [`Self::attach_bridge`].
    /// `None` until a bridge is wired; forwarding is skipped when unset.
    bridge: Mutex<Option<Arc<dyn EventBusBridge>>>,
    /// Best-effort counter of bridge-forward failures, exposed for
    /// diagnostics + the `event_v2_bridge_forward_failed_total` metric
    /// called out in [`crate::bridge`].
    bridge_forward_failures: std::sync::atomic::AtomicU64,
}

impl std::fmt::Debug for InMemoryEventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryEventBus")
            .field("cap", &self.cap)
            .field("ring_len", &self.ring.lock().len())
            .field("closed", &*self.closed.lock())
            .field("bridge_attached", &self.bridge.lock().is_some())
            .field(
                "bridge_forward_failures",
                &self
                    .bridge_forward_failures
                    .load(std::sync::atomic::Ordering::SeqCst),
            )
            .finish_non_exhaustive()
    }
}

impl InMemoryEventBus {
    /// Construct a bus with the default cap (1024).
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_IN_MEMORY_CAP)
    }

    /// Construct a bus with an explicit cap (coerced to `>= 1`).
    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        let effective = cap.max(1);
        Self {
            cap: effective,
            ring: Mutex::new(VecDeque::with_capacity(effective)),
            sequences: Mutex::new(HashMap::new()),
            closed: Mutex::new(false),
            bridge: Mutex::new(None),
            bridge_forward_failures: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Attach a downstream bridge. Subsequent `emit` calls forward
    /// each admitted snapshot to `bridge.forward(&snapshot)` after the
    /// ring push (PR-1.5). Bridge errors are non-fatal: the ring entry
    /// stays and `bridge_forward_failed_count` is incremented. Calling
    /// `attach_bridge` twice replaces the previous bridge.
    pub fn attach_bridge(&self, bridge: Arc<dyn EventBusBridge>) {
        *self.bridge.lock() = Some(bridge);
    }

    /// Number of bridge-forward failures observed since construction.
    /// Exposed for diagnostics + the `event_v2_bridge_forward_failed_total`
    /// metric called out in [`crate::bridge`].
    #[must_use]
    pub fn bridge_forward_failed_count(&self) -> u64 {
        self.bridge_forward_failures
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Current length of the ring.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ring.lock().len()
    }

    /// Whether the ring is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ring.lock().is_empty()
    }

    /// Logical capacity the sink was constructed with.
    #[must_use]
    pub fn cap(&self) -> usize {
        self.cap
    }

    /// Snapshot of the ring contents in emission order.
    #[must_use]
    pub fn snapshot(&self) -> Vec<StoredEventSnapshot> {
        self.ring
            .lock()
            .iter()
            .map(StoredEventSnapshot::from)
            .collect()
    }
}

impl Default for InMemoryEventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Read-only snapshot of a stored event, used for diagnostics and the
/// PR-1.5 `aggregate_events` facade.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StoredEventSnapshot {
    /// Source the event originated from.
    pub source: EventSource,
    /// Sequence number assigned at emit time.
    pub sequence: u64,
    /// 32-byte prefix hash.
    pub prefix_hash: PrefixHash,
    /// Creation timestamp (ms since Unix epoch).
    pub created_at_ms: i64,
    /// Raw JSON payload.
    pub payload: serde_json::Value,
}

impl From<&StoredEvent> for StoredEventSnapshot {
    fn from(e: &StoredEvent) -> Self {
        Self {
            source: e.version.source,
            sequence: e.version.sequence,
            prefix_hash: e.prefix_hash,
            created_at_ms: e.created_at_ms,
            payload: e.payload.clone(),
        }
    }
}

#[async_trait::async_trait]
impl EventBus for InMemoryEventBus {
    fn kind(&self) -> EventSinkKind {
        EventSinkKind::InMemory
    }

    async fn emit<P>(&self, payload: &P) -> Result<EmitOutcome, EventBusError>
    where
        P: Serialize + Sync + Send + 'static,
    {
        if *self.closed.lock() {
            return Err(EventBusError::Closed);
        }
        let payload_json = serde_json::to_value(payload)?;

        // Per-source monotonic sequence allocation. The default source
        // is `System`; PR-1.5's `aggregate_events::<T>()` facade threads
        // through the real prefix-hash inputs.
        let source = EventSource::System;
        let mut sequences = self.sequences.lock();
        let seq = sequences.entry(source).or_default();
        let sequence = seq.next();
        drop(sequences);

        let version = EventVersion { source, sequence };
        let meta = EventMeta::from_prefix_hex(
            source,
            sequence,
            &"00".repeat(32),
            None,
        );
        let _envelope: EventEnvelope<serde_json::Value> =
            EventEnvelope::new(version, meta.clone(), payload_json.clone());

        let entry = StoredEvent {
            version,
            prefix_hash: PrefixHash::default(),
            created_at_ms: meta.created_at_ms,
            payload: payload_json,
        };
        let snapshot = StoredEventSnapshot::from(&entry);

        let mut ring = self.ring.lock();
        if ring.len() >= self.cap {
            ring.pop_front();
            tracing::trace!(
                target: "synthia::event_v2",
                sink = "in_memory",
                cap = self.cap,
                "evicted oldest event (ring full)",
            );
        }
        ring.push_back(entry);
        drop(ring);

        // Bridge forwarding (PR-1.5). Clone the Arc out of the lock so
        // the bridge call (which may do I/O under a `mpsc::Sender`'s
        // internal `Mutex`) does not hold the parking_lot mutex for the
        // entire duration.
        if let Some(bridge) = self.bridge.lock().clone()
            && let Err(e) = bridge.forward(&snapshot)
        {
            self.bridge_forward_failures
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            tracing::trace!(
                target: "synthia::event_v2",
                sink = "in_memory",
                bridge = bridge.label(),
                sequence = snapshot.sequence,
                error = %e,
                "bridge forward failed; ring entry retained",
            );
        }

        Ok(EmitOutcome::Buffered)
    }

    fn is_closed(&self) -> bool {
        *self.closed.lock()
    }
}

impl Drop for InMemoryEventBus {
    fn drop(&mut self) {
        // Mark the bus closed so any concurrent `emit` returns `Closed`.
        *self.closed.lock() = true;
        // Drain the ring and report the lifetime totals in a single
        // trace span so consumers (`synthia-telemetry`) can record the
        // event throughput via standard tracing subscribers.
        let drained = self.ring.lock().drain(..).count();
        tracing::trace!(
            target: "synthia::event_v2",
            sink = "in_memory",
            drained,
            cap = self.cap,
            "in-memory event bus dropped; drained ring",
        );
    }
}

/// Convenience constructor kept for backwards compatibility with the
/// PR-1.1 acceptance tests (`pr_1_1_default_sink_is_in_memory` etc.).
#[must_use]
pub fn bus() -> Arc<InMemoryEventBus> {
    Arc::new(InMemoryEventBus::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_cap_is_1024() {
        let bus = InMemoryEventBus::new();
        assert_eq!(bus.cap(), DEFAULT_IN_MEMORY_CAP);
        assert_eq!(bus.cap(), 1024);
        assert!(bus.is_empty());
    }

    #[test]
    fn with_capacity_coerces_zero() {
        let bus = InMemoryEventBus::with_capacity(0);
        assert_eq!(bus.cap(), 1);
    }

    #[test]
    fn closed_flag_default_false() {
        let bus = InMemoryEventBus::new();
        assert!(!bus.is_closed());
    }
}
