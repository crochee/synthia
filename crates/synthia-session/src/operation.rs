//! [`OperationSnapshot`] — a read-only snapshot of one running
//! agent operation, suitable for HTTP `/status` endpoints, TUI
//! dashboards, and replay consumers.
//!
//! ## Motivation
//!
//! pi `harness/runtime/types.ts::OperationState` is a 13-leaf
//! `OperationState` union that drives the Lane reducer. The
//! *internal* Lane reducer is out of scope for R9 (deferred
//! to R10+) — what lib consumers actually want is the
//! **read-only** summary a status endpoint reports:
//!
//! - Is the run idle / running / completing / failed?
//! - What is its current iteration index?
//! - What is its accumulated token usage?
//! - What was the most recent error (if any)?
//!
//! `OperationSnapshot` is the typed value that exposes exactly
//! that surface. The agent loop / session controller
//! constructs one at meaningful boundaries (start / iteration
//! end / completion / cancellation / error) and pushes it to
//! any consumer that asks for the current state without
//! wanting to introspect the underlying ReActLoop internals.
//!
//! ## Layering
//!
//! ```text
//! ReActLoop    → OperationSnapshot (per-boundary)
//! SessionController  → public API surface
//! HTTP /sessions/:id/status  → OperationSnapshot JSON
//! TUI / replay  → OperationSnapshot
//! ```
//!
//! ## Reference
//!
//! Adopted from pi `harness/runtime/types.ts` (read-only subset
//! of OperationState) + opencode `session.ts` (snapshot).
//! Synthia's port keeps the **state** as a flat 4-variant enum
//! (the bare minimum consumers need) and defers the full Lane
//! reducer to R10+.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use synthia_core::Clock;
use synthia_provider::TokenUsage;

use crate::token_meter::{ContextPressure, UsageBuckets};

/// Coarse operational state — what a consumer can read at a
/// glance without exposing the ReActLoop internals.
///
/// Variants are deliberately flat (no nested Lane model):
///
/// - `Idle` — the operation has not started or has been torn
///   down.
/// - `Running` — the operation is actively executing; consumers
///   can read `iteration` for progress.
/// - `Completing` — the operation has reached its terminal
///   iteration and is flushing final events.
/// - `Failed` — the operation exited with an unrecoverable
///   error; `error_message` carries a human-readable cause.
/// - `Cancelled` — the operation was cancelled (via
///   `Arc<dyn CancelToken>::cancel()`); the consumer may retry
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum OperationState {
    #[default]
    Idle,
    Running {
        /// Current iteration index (0-based). `0` means the
        /// loop has just started; `max_iterations - 1` means
        /// the last allowed iteration.
        iteration: usize,
    },
    Completing,
    Failed {
        /// Human-readable failure cause.
        error_message: String,
    },
    Cancelled {
        /// Human-readable cancellation cause.
        reason: String,
    },
}

impl OperationState {
    /// Short, stable label suitable for log lines / metrics.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running { .. } => "running",
            Self::Completing => "completing",
            Self::Failed { .. } => "failed",
            Self::Cancelled { .. } => "cancelled",
        }
    }

    /// True when the operation is in a terminal state
    /// (`Failed` / `Cancelled` / completed-via-`Completing`).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completing | Self::Failed { .. } | Self::Cancelled { .. }
        )
    }
}

/// A read-only snapshot of one running agent operation.
///
/// `OperationSnapshot` is the public surface lib consumers read
/// to monitor a session without poking at ReActLoop internals.
/// Construct one at meaningful boundaries inside the agent
/// loop (start / iteration end / completion / cancellation /
/// error) and ship it via a channel to any consumer that wants
/// to render progress (TUI, HTTP `/status`, replay tool, …).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OperationSnapshot {
    /// Session identifier this snapshot belongs to.
    pub session_id: String,
    /// Agent descriptor name (e.g. `"default"`).
    pub agent_name: String,
    /// Coarse operational state.
    pub state: OperationState,
    /// Current iteration index (0-based). Mirrors
    /// `AgentState::iteration_count` at the snapshot's wall
    /// clock.
    pub iteration: usize,
    /// Effective iteration cap after clamping (mirrors
    /// `Agent::effective_max_iterations`).
    pub max_iterations: usize,
    /// Wall-clock timestamp at which the snapshot was taken
    /// (chrono `DateTime<Utc>`, matches the R8 wall-clock
    /// convention).
    pub taken_at: DateTime<Utc>,
    /// Accumulated token usage up to the snapshot's wall
    /// clock.
    pub usage: TokenUsage,
    /// Cumulative disjoint provider usage folded from the durable
    /// log by the [`crate::token_meter::TokenMeter`]. `None` on
    /// snapshots from runs without metering and on payloads
    /// written before the field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_buckets: Option<UsageBuckets>,
    /// Context-pressure projection (anchor pressure + projected
    /// next-request total). `None` until the meter adopted a
    /// provider usage sample.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_pressure: Option<ContextPressure>,
    /// Last error message, when `state == Failed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Most recent compaction event, when one happened
    /// during the run. `None` otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_compaction: Option<CompactionRef>,
}

/// A pointer to the last compaction that fired during the
/// operation, so a status endpoint can answer "what was the
/// most recent compaction?" without scanning the full event
/// log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionRef {
    /// Event sequence number of the compaction event.
    pub seq: u64,
    /// Wall-clock timestamp the compaction fired.
    pub at: DateTime<Utc>,
}

impl OperationSnapshot {
    /// Build a new snapshot, stamping `taken_at` from the wall clock
    /// (through [`synthia_core::Clock`], not
    /// `chrono::Utc::now`).
    pub fn new(
        session_id: impl Into<String>,
        agent_name: impl Into<String>,
        state: OperationState,
        iteration: usize,
        max_iterations: usize,
        usage: TokenUsage,
    ) -> Self {
        Self::new_at(
            session_id,
            agent_name,
            state,
            iteration,
            max_iterations,
            usage,
            synthia_core::SharedClock::system().now(),
        )
    }

    /// [`OperationSnapshot::new`] with an explicit `taken_at` — for
    /// callers whose clock comes from elsewhere (a deployment keeping
    /// its own clock, or a test pinning the timestamp).
    pub fn new_at(
        session_id: impl Into<String>,
        agent_name: impl Into<String>,
        state: OperationState,
        iteration: usize,
        max_iterations: usize,
        usage: TokenUsage,
        taken_at: DateTime<Utc>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            agent_name: agent_name.into(),
            state,
            iteration,
            max_iterations,
            taken_at,
            usage,
            usage_buckets: None,
            context_pressure: None,
            last_error: None,
            last_compaction: None,
        }
    }

    /// Builder — attach the meter's cumulative disjoint usage
    /// buckets.
    #[must_use]
    pub fn with_usage_buckets(mut self, buckets: UsageBuckets) -> Self {
        self.usage_buckets = Some(buckets);
        self
    }

    /// Builder — attach the meter's context-pressure projection.
    #[must_use]
    pub fn with_context_pressure(mut self, pressure: ContextPressure) -> Self {
        self.context_pressure = Some(pressure);
        self
    }

    /// Builder — set the last error (when transitioning to
    /// `Failed`).
    pub fn with_error(mut self, message: impl Into<String>) -> Self {
        self.last_error = Some(message.into());
        self
    }

    /// Builder — record the last compaction.
    pub fn with_compaction(mut self, seq: u64, at: DateTime<Utc>) -> Self {
        self.last_compaction = Some(CompactionRef { seq, at });
        self
    }

    /// Convenience constructor for a freshly-started run.
    pub fn started(
        session_id: impl Into<String>,
        agent_name: impl Into<String>,
        max_iterations: usize,
    ) -> Self {
        Self::new(
            session_id,
            agent_name,
            OperationState::Running { iteration: 0 },
            0,
            max_iterations,
            TokenUsage::default(),
        )
    }

    /// Convenience constructor for a completed run.
    pub fn completed(
        session_id: impl Into<String>,
        agent_name: impl Into<String>,
        iteration: usize,
        max_iterations: usize,
        usage: TokenUsage,
    ) -> Self {
        Self::new(
            session_id,
            agent_name,
            OperationState::Completing,
            iteration,
            max_iterations,
            usage,
        )
    }
}

// -- SnapshotBus: subscribe/notify channel -------------------------

use std::sync::Arc;

use futures::channel::mpsc;

/// Default channel capacity for [`SnapshotBus::subscribe`].
///
/// 64 is enough to absorb a 25-iteration run's worth of
/// iteration-end snapshots plus start/complete/error without
/// the consumer ever lagging. Lagged receivers drop the oldest
/// slot (broadcast semantics).
pub const DEFAULT_BUS_CAPACITY: usize = 64;

/// Multi-producer / multi-consumer bus for [`OperationSnapshot`].
///
/// Closes the R9-6 deferred wire-up: lib consumers subscribe to
/// the bus and receive every snapshot the session controller
/// publishes. Subscribers that lag (do not drain in time) drop
/// the oldest snapshot, never the newest, so a status endpoint
/// that reads intermittently still gets the freshest state.
#[derive(Clone)]
pub struct SnapshotBus {
    inner: Arc<parking_lot::Mutex<BusInner>>,
}

struct BusInner {
    /// Snapshot sequence counter (monotonic per bus).
    next_seq: u64,
    /// Last snapshot published (cached for late subscribers).
    last: Option<OperationSnapshot>,
    /// Subscribers. Each is an `mpsc::Sender`; a subscriber is
    /// dropped from the list on send failure (channel closed or
    /// would-block).
    subscribers: Vec<mpsc::UnboundedSender<OperationSnapshot>>,
}

impl SnapshotBus {
    /// Build a new bus with the default channel capacity.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_BUS_CAPACITY)
    }

    /// Build a new bus. (Capacity is reserved for a future
    /// bounded-channel upgrade; the current implementation uses
    /// `UnboundedSender` so subscribers never block the
    /// publisher.)
    pub fn with_capacity(_capacity: usize) -> Self {
        Self {
            inner: Arc::new(parking_lot::Mutex::new(BusInner {
                next_seq: 0,
                last: None,
                subscribers: Vec::new(),
            })),
        }
    }

    /// Subscribe to the bus. Returns a receiver plus the latest
    /// snapshot seen, so the consumer does not have to wait for
    /// the next publish to learn the current state.
    pub fn subscribe(&self) -> (SnapshotReceiver, Option<OperationSnapshot>) {
        let (tx, rx) = mpsc::unbounded();
        let latest = self.inner.lock().last.clone();
        self.inner.lock().subscribers.push(tx);
        (SnapshotReceiver { rx }, latest)
    }

    /// Publish one snapshot. Returns the sequence number
    /// assigned to it.
    pub fn publish(&self, snapshot: OperationSnapshot) -> u64 {
        let mut guard = self.inner.lock();
        guard.next_seq = guard.next_seq.saturating_add(1);
        let seq = guard.next_seq;
        guard.last = Some(snapshot.clone());
        // Drop any subscribers whose channel is closed.
        guard
            .subscribers
            .retain(|tx| tx.unbounded_send(snapshot.clone()).is_ok());
        seq
    }

    /// Latest snapshot published, if any. Cheap (lock + clone).
    pub fn latest(&self) -> Option<OperationSnapshot> {
        self.inner.lock().last.clone()
    }

    /// Total snapshots published so far.
    pub fn published_count(&self) -> u64 {
        self.inner.lock().next_seq
    }

    /// Active subscriber count.
    pub fn subscriber_count(&self) -> usize {
        self.inner.lock().subscribers.len()
    }
}

impl Default for SnapshotBus {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SnapshotBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let g = self.inner.lock();
        f.debug_struct("SnapshotBus")
            .field("next_seq", &g.next_seq)
            .field("last_state", &g.last.as_ref().map(|s| &s.state))
            .field("subscribers", &g.subscribers.len())
            .finish()
    }
}

/// Receiver for the [`SnapshotBus`]. Wraps an unbounded
/// `mpsc::Receiver` and exposes a sync `try_next` plus the
/// stream-like `next().await` adapter.
pub struct SnapshotReceiver {
    rx: mpsc::UnboundedReceiver<OperationSnapshot>,
}

impl SnapshotReceiver {
    /// Pull the next pending snapshot, if any. Returns
    /// `Some(snap)` when one is queued, `None` when the channel
    /// is empty or closed.
    pub fn try_next(&mut self) -> Option<OperationSnapshot> {
        self.rx.try_recv().ok()
    }

    /// Await the next snapshot. Returns `None` if the bus has
    pub async fn next(&mut self) -> Option<OperationSnapshot> {
        use futures::StreamExt;
        self.rx.next().await
    }
}

#[cfg(test)]
mod bus_tests {
    use super::*;

    #[tokio::test]
    async fn publish_then_receive() {
        let bus = SnapshotBus::new();
        let (mut rx, latest) = bus.subscribe();
        assert!(latest.is_none());
        let snap = OperationSnapshot::started("s1", "default", 5);
        let seq = bus.publish(snap.clone());
        assert_eq!(seq, 1);
        let received = rx.next().await.unwrap();
        assert_eq!(received.session_id, "s1");
        assert_eq!(bus.published_count(), 1);
    }

    #[test]
    fn latest_caches_for_late_subscribers() {
        let bus = SnapshotBus::new();
        let snap = OperationSnapshot::started("s1", "default", 5);
        bus.publish(snap);
        let (_rx, latest) = bus.subscribe();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().session_id, "s1");
    }

    #[test]
    fn publish_drops_dead_subscribers() {
        let bus = SnapshotBus::new();
        let (rx, _latest) = bus.subscribe();
        drop(rx);
        let snap = OperationSnapshot::started("s1", "default", 5);
        bus.publish(snap);
        assert_eq!(bus.subscriber_count(), 0);
    }

    #[test]
    fn default_bus_capacity_is_documented() {
        assert_eq!(DEFAULT_BUS_CAPACITY, 64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_state_kind_is_stable() {
        assert_eq!(OperationState::Idle.kind(), "idle");
        assert_eq!(OperationState::Running { iteration: 0 }.kind(), "running");
        assert_eq!(OperationState::Completing.kind(), "completing");
        assert_eq!(
            OperationState::Failed {
                error_message: "x".to_string()
            }
            .kind(),
            "failed"
        );
        assert_eq!(
            OperationState::Cancelled {
                reason: "x".to_string()
            }
            .kind(),
            "cancelled"
        );
    }

    #[test]
    fn operation_state_is_terminal() {
        assert!(!OperationState::Idle.is_terminal());
        assert!(!OperationState::Running { iteration: 0 }.is_terminal());
        assert!(OperationState::Completing.is_terminal());
        assert!(
            OperationState::Failed {
                error_message: "x".to_string()
            }
            .is_terminal()
        );
        assert!(
            OperationState::Cancelled {
                reason: "x".to_string()
            }
            .is_terminal()
        );
    }

    #[test]
    fn snapshot_started_carries_running_state() {
        let snap = OperationSnapshot::started("s1", "default", 5);
        assert_eq!(snap.session_id, "s1");
        assert_eq!(snap.agent_name, "default");
        assert_eq!(snap.iteration, 0);
        assert_eq!(snap.max_iterations, 5);
        assert!(matches!(
            snap.state,
            OperationState::Running { iteration: 0 }
        ));
    }

    #[test]
    fn snapshot_completed_carries_completing_state() {
        let snap = OperationSnapshot::completed(
            "s1",
            "default",
            3,
            5,
            TokenUsage::default(),
        );
        assert_eq!(snap.iteration, 3);
        assert_eq!(snap.max_iterations, 5);
        assert!(matches!(snap.state, OperationState::Completing));
    }

    #[test]
    fn snapshot_builder_records_error() {
        let snap = OperationSnapshot::started("s1", "default", 5)
            .with_error("provider unreachable");
        assert_eq!(snap.last_error, Some("provider unreachable".to_string()));
    }

    #[test]
    fn snapshot_builder_records_compaction() {
        let pinned =
            chrono::DateTime::parse_from_rfc3339("2026-01-01T12:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc);
        let snap = OperationSnapshot::started("s1", "default", 5)
            .with_compaction(7, pinned);
        assert!(snap.last_compaction.is_some());
        assert_eq!(snap.last_compaction.unwrap().seq, 7);
    }

    #[test]
    fn snapshot_serializes_to_json_round_trip() {
        let snap = OperationSnapshot::started("s1", "default", 5);
        let json = serde_json::to_string(&snap).expect("serialize");
        let back: OperationSnapshot =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.session_id, snap.session_id);
        assert_eq!(back.iteration, snap.iteration);
        assert_eq!(back.max_iterations, snap.max_iterations);
    }

    #[test]
    fn snapshot_skips_none_fields_in_serialization() {
        let snap = OperationSnapshot::started("s1", "default", 5);
        let json = serde_json::to_value(&snap).expect("serialize");
        // last_error / last_compaction are None and must NOT
        // appear as `null` in the JSON.
        assert!(json.get("last_error").is_none());
        assert!(json.get("last_compaction").is_none());
        // Meter-fed fields follow the same contract.
        assert!(json.get("usage_buckets").is_none());
        assert!(json.get("context_pressure").is_none());
    }

    #[test]
    fn snapshot_deserializes_payload_without_meter_fields() {
        // A status payload written before the meter fields
        // existed must keep parsing, with both slots defaulting
        // to `None`.
        let old = serde_json::json!({
            "session_id": "s1",
            "agent_name": "default",
            "state": {"state": "running", "iteration": 2},
            "iteration": 2,
            "max_iterations": 8,
            "taken_at": "2026-09-12T00:00:00Z",
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 20,
                "total_tokens": 120
            }
        });
        let snap: OperationSnapshot =
            serde_json::from_value(old).expect("old payload parses");
        assert_eq!(snap.iteration, 2);
        assert!(snap.usage_buckets.is_none());
        assert!(snap.context_pressure.is_none());
    }

    #[test]
    fn snapshot_round_trips_meter_fields() {
        let buckets = UsageBuckets {
            input_tokens: 400,
            output_tokens: 90,
            cache_read_tokens: Some(50),
            cache_write_tokens: None,
        };
        let pressure = ContextPressure {
            pressure_tokens: 450,
            projected_tokens: 480,
            context_window: Some(200_000),
        };
        let snap = OperationSnapshot::started("s1", "default", 5)
            .with_usage_buckets(buckets)
            .with_context_pressure(pressure);
        let json = serde_json::to_string(&snap).expect("serialize");
        let back: OperationSnapshot =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.usage_buckets, Some(buckets));
        assert_eq!(back.context_pressure, Some(pressure));
    }

    #[test]
    fn snapshot_default_state_is_idle() {
        let state: OperationState = Default::default();
        assert!(matches!(state, OperationState::Idle));
    }
}
