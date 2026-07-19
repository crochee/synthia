//! `synthia-event-v2` — Event Sourcing bus for Synthia.
//!
//! ## Phase A scaffolding (PR-1.1 → PR-1.5, change #1)
//!
//! PR-1.1 (this file): `EventBus` trait + `EventSinkKind` enum + module paths.
//! PR-1.2: `EventEnvelope<T>`, `EventVersion`, `EventMeta` populated
//!      (see [`event`]).
//! PR-1.3: `InMemory` sink implementation (bounded ring 1024, see
//!      [`sink::in_memory`]).
//! PR-1.4: `Sqlite` sink implementation behind the `sqlite` feature flag
//!      (see [`sink::sqlite`] when compiled with `--features sqlite`).
//! PR-1.5: `Projector` + `CommitGuard` + `aggregate_events::<T>()` facade
//!      + `EventBusBridge` trait + `MpscEventBridge` default impl
//!      + optional `GrpcEventBridge` stub behind the `grpc-bridge` feature.
//!
//! The default build (`default-features = true`) carries only the
//! in-memory sink + the typed projection surface and depends only on
//! already-workspace-pinned crates. The `sqlite` and `grpc-bridge`
//! features are opt-in.
//!
//! See `openspec/changes/2026-07-18-synthia-top5-borrow-integration/`
//! (spec `event-v2-system`) for the normative requirements.

#![allow(clippy::missing_const_for_fn)] // PR-1.1 scaffold: const-fns land with PR-1.2.

pub mod aggregate;
pub mod bridge;
pub mod cleanup;
pub mod commit_guard;
pub mod event;
pub mod projector;
pub mod sink;

pub use aggregate::{
    AggregateError,
    aggregate_events,
    aggregate_events_default,
};
#[cfg(feature = "grpc-bridge")]
pub use bridge::GrpcEventBridge;
pub use bridge::{BridgeError, EventBusBridge, MpscEventBridge};
pub use commit_guard::{CommitGuard, CommitGuardError, Rule};
pub use projector::{IdentityProjector, Projector, ProjectorError};

/// Identifies the physical sink backing an [`EventBus`].
///
/// The in-memory variant is always available. The SQLite variant is gated
/// behind the `sqlite` Cargo feature; see PR-1.4 in `tasks.md`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum EventSinkKind {
    /// Bounded ring buffer in process memory. Default sink.
    #[default]
    InMemory,
    /// Durable dual-table SQLite backend. Requires the `sqlite` feature.
    #[cfg(feature = "sqlite")]
    Sqlite,
}

impl EventSinkKind {
    /// Human-readable name, used in tracing fields and metrics labels.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InMemory => "in_memory",
            #[cfg(feature = "sqlite")]
            Self::Sqlite => "sqlite",
        }
    }
}

/// Errors that can surface from the [`EventBus`] scaffold.
///
/// PR-1.1 only enumerates the variants downstream implementations will need;
/// populated variants land with PR-1.2 and later.
#[derive(Debug, thiserror::Error)]
pub enum EventBusError {
    /// The bus is shutting down or has been dropped.
    #[error("event bus closed")]
    Closed,
    /// The supplied payload failed to serialize into the envelope.
    #[error("event payload serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
    /// A sink-specific error occurred (e.g. SQLite I/O).
    #[error("event sink error: {0}")]
    Sink(String),
}

/// Outcome of [`EventBus::emit`].
///
/// `Emit` is fire-and-forget on the default in-memory sink; the `Persisted`
/// variant is reserved for the durable sink in PR-1.4 and is reported by
/// the `sqlite` feature only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EmitOutcome {
    /// The event was buffered; downstream subscribers will observe it in
    /// delivery order.
    Buffered,
    /// The event was durably persisted (only with the `sqlite` feature).
    #[cfg(feature = "sqlite")]
    Persisted,
}

/// The trait surface every bus implementation must satisfy.
///
/// PR-1.1 carries only the method signatures; the return types are
/// intentionally narrow (`()` for `subscribe`, a counted `Buffered` for
/// `emit`) and will be widened as PR-1.2 lands `EventEnvelope<T>`.
#[async_trait::async_trait]
pub trait EventBus: Send + Sync + 'static {
    /// Returns the sink backing this bus.
    fn kind(&self) -> EventSinkKind;

    /// Submits an event to the bus.
    ///
    /// `payload` is anything `serde::Serialize`. The bus is responsible
    /// for serializing it into the envelope's data field (PR-1.2).
    async fn emit<P>(&self, payload: &P) -> Result<EmitOutcome, EventBusError>
    where
        P: serde::Serialize + Sync + Send + 'static;

    /// Returns `true` once the bus has been closed and will reject future
    /// `emit` calls. PR-1.3 implements the actual close path; the default
    /// here is `false` so PR-1.1 cannot accidentally signal shutdown.
    fn is_closed(&self) -> bool {
        false
    }
}
