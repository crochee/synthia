//! `EventBusBridge` trait + default impls (PR-1.5).
//!
//! A `bridge` consumes `StoredEventSnapshot`s produced by the
//! in-memory bus and forwards them to a downstream subscriber (the
//! existing gRPC `synthia-message-proxy`, an in-process `mpsc`
//! channel, or — by composing users — a custom sink).
//!
//! ## Trait surface
//!
//! `EventBusBridge::forward(&self, snapshot)` is called by the bus
//! *after* the event has been admitted to the ring. Returning `Err`
//! does NOT roll back the ring entry — the bus treats the bridge as
//! best-effort and the failure is recorded in the metrics counter
//! described in `specs/event-v2-system/spec.md` ("Scenario: grpc
//! downstream fanout").
//!
//! ## Default impls
//!
//! - [`MpscEventBridge`] — `tokio::sync::mpsc::Sender<serde_json::Value>`
//!   channel. Default impl, always available, no external deps. Use
//!   this for in-process subscribers and as the smoke bridge for the
//!   `gRPC downstream fanout` scenario in `specs/event-v2-system/spec.md`.
//! - `GrpcEventBridge` — feature-gated stub (`grpc-bridge` feature).
//!   The actual tonic wire transport lands in change #4 (server /
//!   protocol / MCP). PR-1.5 ships the trait seat so downstream
//!   consumers can wire a real `tonic` bridge without an API break.

use std::sync::Arc;

use thiserror::Error;

use crate::sink::in_memory::StoredEventSnapshot;

/// Bridge-adapter errors.
#[derive(Debug, Error)]
pub enum BridgeError {
    /// The downstream sink is closed (e.g. the mpsc receiver was dropped).
    #[error("event bridge downstream closed")]
    DownstreamClosed,
    /// Serialization of the snapshot failed.
    #[error("event bridge serde error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Forwarding surface that bridges a snapshot downstream.
pub trait EventBusBridge: Send + Sync + 'static {
    /// Forward a snapshot. Returning `Err` does NOT abort the bus.
    fn forward(
        &self,
        snapshot: &StoredEventSnapshot,
    ) -> Result<(), BridgeError>;

    /// Human-readable label for tracing (`"mpsc"`, `"grpc"`, ...).
    fn label(&self) -> &'static str;
}

/// Default in-process bridge: an mpsc channel that serializes each
/// snapshot to JSON and pushes it.
pub struct MpscEventBridge {
    label: &'static str,
    tx: tokio::sync::mpsc::Sender<serde_json::Value>,
}

impl std::fmt::Debug for MpscEventBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MpscEventBridge")
            .field("label", &self.label)
            .field("tx_capacity", &self.tx.capacity())
            .finish_non_exhaustive()
    }
}

impl MpscEventBridge {
    /// Construct an `MpscEventBridge` paired with a freshly-created
    /// receiver of capacity `cap`.
    #[must_use]
    pub fn new(
        cap: usize,
    ) -> (Arc<Self>, tokio::sync::mpsc::Receiver<serde_json::Value>) {
        let (tx, rx) = tokio::sync::mpsc::channel(cap);
        (Arc::new(Self { label: "mpsc", tx }), rx)
    }

    /// Wrap an existing `Sender` (useful when the receiver is owned
    /// elsewhere).
    #[must_use]
    pub fn with_sender(
        tx: tokio::sync::mpsc::Sender<serde_json::Value>,
    ) -> Arc<Self> {
        Arc::new(Self { label: "mpsc", tx })
    }
}

impl EventBusBridge for MpscEventBridge {
    fn forward(
        &self,
        snapshot: &StoredEventSnapshot,
    ) -> Result<(), BridgeError> {
        // Best-effort blocking-send with bounded depth: a closed
        // downstream surfaces as `DownstreamClosed` so the bus can
        // log + increment metrics instead of panicking.
        let body = serde_json::to_value(snapshot)?;
        self.tx
            .try_send(body)
            .map_err(|_| BridgeError::DownstreamClosed)
    }

    fn label(&self) -> &'static str {
        self.label
    }
}

// -----------------------------------------------------------------------------
// Feature-gated gRPC bridge seat
// -----------------------------------------------------------------------------
//
// `GrpcEventBridge` is a trait seat only in PR-1.5 — the actual tonic
// wire transport (proto schema, tonic-build, transport) lives in
// change #4 (`server/cli/protocol/MCP`). PR-1.5 ships the
// `grpc-bridge` feature as an opt-in flag so callers compiling with
// `--features synthia-event-v2/grpc-bridge` get the seat without an
// API break when the real wire format lands.

#[cfg(feature = "grpc-bridge")]
mod grpc_bridge {
    use std::sync::Arc;

    use super::{BridgeError, EventBusBridge, StoredEventSnapshot};

    /// gRPC bridge seat (PR-1.5 stub; wire transport in change #4).
    #[derive(Debug, Default)]
    pub struct GrpcEventBridge {
        /// Label reported via [`EventBusBridge::label`].
        label: &'static str,
    }

    impl GrpcEventBridge {
        /// Construct a stub `GrpcEventBridge`. The actual tonic
        /// transport is added in change #4 alongside the rest of the
        /// synthia-message-proxy wire format.
        #[must_use]
        pub fn new() -> Arc<Self> {
            Arc::new(Self { label: "grpc" })
        }
    }

    impl EventBusBridge for GrpcEventBridge {
        fn forward(
            &self,
            _snapshot: &StoredEventSnapshot,
        ) -> Result<(), BridgeError> {
            // No-op until change #4 wires the real tonic transport.
            Ok(())
        }

        fn label(&self) -> &'static str {
            self.label
        }
    }
}

#[cfg(feature = "grpc-bridge")]
pub use grpc_bridge::GrpcEventBridge;

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::event::{EventSource, PrefixHash};

    fn snap(
        source: EventSource,
        sequence: u64,
        payload: serde_json::Value,
    ) -> StoredEventSnapshot {
        StoredEventSnapshot {
            source,
            sequence,
            prefix_hash: PrefixHash::default(),
            created_at_ms: 0,
            payload,
        }
    }

    #[tokio::test]
    async fn mpsc_bridge_forwards_snapshots() {
        let (bridge, mut rx) = MpscEventBridge::new(8);
        let payload = json!({"k": "v"});
        let snapshot = snap(EventSource::System, 1, payload);
        let expected =
            serde_json::to_value(&snapshot).expect("snapshot serializes");
        bridge.forward(&snapshot).expect("forward must succeed");
        let received = rx.recv().await.expect("receiver must observe snapshot");
        assert_eq!(received, expected);
        assert_eq!(bridge.label(), "mpsc");
    }

    #[tokio::test]
    async fn mpsc_bridge_returns_downstream_closed_when_receiver_dropped() {
        let (bridge, rx) = MpscEventBridge::new(1);
        drop(rx);
        let err = bridge
            .forward(&snap(EventSource::System, 1, json!({})))
            .expect_err("forward must fail when downstream is closed");
        assert!(matches!(err, BridgeError::DownstreamClosed));
    }

    #[test]
    fn mpsc_bridge_with_existing_sender() {
        let (tx, _rx) = tokio::sync::mpsc::channel::<serde_json::Value>(4);
        let bridge = MpscEventBridge::with_sender(tx);
        assert_eq!(bridge.label(), "mpsc");
    }
}
