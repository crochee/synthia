//! `SessionSink` — the **only** public surface of `synthia-session`.
//!
//! ## Why so small?
//!
//! The session crate used to expose ~30 public types
//! (`SessionManager`, `Store`, `Session`, `SessionStateMachine`,
//! `SessionInputQueue`, `EventStore`, …) plus state-machine and
//! token-budget modules. That surface bled orchestration policy
//! (approval workflows, status transitions, cache eviction) into a
//! module called "session", which broke the layering between the
//! agent runtime (stateless, streaming) and the session storage
//! (write-through, inert).
//!
//! After this refactor:
//!
//! - `SessionSink` is the only trait the agent runtime imports.
//! - It has exactly **5 methods** (`id`, `append`, `read`,
//!   `snapshot`, `close`). Anything else the caller wants to do
//!   with a session is **policy** and lives outside this crate
//!   (typically in `synthia-server`).
//! - All on-disk persistence, state machines, approval flows, and
//!   token-budget tracking have been either deleted or moved into
//!   the `synthia-server::session` orchestration layer.
//!
//! ## Event shape
//!
//! `SessionSink` is event-shape-agnostic: it stores opaque
//! `serde_json::Value` records. Callers (`synthia-agent`) are
//! responsible for serializing their own events before calling
//! `append`. This keeps the dependency direction strictly
//! `agent → session` (never the reverse) and lets the same sink
//! back agents with different event taxonomies (ReAct, planner,
//! …) without trait churn.
//!
//! ## Semantics
//!
//! ### Write-through
//!
//! `append` returns `Ok(())` only when the event is durable (or
//! the implementation has explicit transactional semantics, such
//! as `InMemorySessionSink`). The agent loop MUST treat
//! `Err(SessionError)` as fatal and stop the run — the call site
//! is responsible for retrying at the request boundary, not at
//! the agent step boundary. This matches the `fail-fast` rule
//! agreed with the user.
//!
//! ### Inert container
//!
//! `SessionSink` is a **mechanism**, not a **policy**. It does
//! NOT track approval state, session state-machine transitions,
//! token budgets, or per-user routing. Callers (the
//! `synthia-server::session::SessionController`) own that
//! bookkeeping on top of the sink's primitives.
//!
//! ### Read parity
//!
//! `read()` MUST return records in chronological order, identical
//! to the order they were passed to `append`. Implementations
//! that batch / compress MUST surface that order in `read()`
//! output. This lets any caller that wants to rehydrate a
//! session under a different agent reconstruct a consistent
//! message stream regardless of the backend.

use std::fmt;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The single trait every agent loop sees.
///
/// `synthia-agent` holds `Arc<dyn SessionSink>`; it never sees
/// concrete backends. This keeps the agent runtime portable and
/// makes test fixtures trivial (`InMemorySessionSink`).
#[async_trait]
pub trait SessionSink: Send + Sync {
    /// Stable, opaque session identifier. Returned by reference so
    /// callers can label trace events without cloning on every
    /// emit.
    fn id(&self) -> &str;

    /// Append one event durably. **Write-through** — when the
    /// future resolves with `Ok`, the bytes are durable (or the
    /// caller has explicit transactional guarantees).
    ///
    /// Failure is fatal at the call site: the agent loop MUST
    /// surface the error to its caller rather than silently
    /// dropping the event.
    async fn append(&self, event: &Value) -> Result<(), SessionError>;

    /// Reconstruct the chronological event stream. Returns events
    /// in the order they were `append`-ed. Used by callers that
    /// want to rehydrate a session (e.g. resume under a
    /// different agent).
    async fn read(&self) -> Result<Vec<Value>, SessionError>;

    /// Force a stable checkpoint (flush buffers, rotate logs,
    /// push to remote). Called when the session enters a stable
    /// state (idle, completed). Returns the snapshot metadata.
    async fn snapshot(&self) -> Result<SessionSnapshot, SessionError>;

    /// Mark the session closed. Idempotent. After this resolves
    /// no further `append` / `read` calls are accepted.
    async fn close(&self, reason: SessionEndReason)
    -> Result<(), SessionError>;
}

/// Outcome category attached to the final `close()` call.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum SessionEndReason {
    /// Run completed normally.
    #[default]
    Completed,
    /// User cancelled the run.
    Cancelled,
    /// Provider error / fatal failure during the run.
    Error,
    /// Server shut down before completion.
    Interrupted,
}

/// Snapshot metadata returned by `SessionSink::snapshot`.
///
/// `last_event_seq` lets callers detect "no new events since last
/// snapshot" without re-reading the entire log. `bytes_on_disk`
/// is informational.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub session_id: String,
    pub last_event_seq: u64,
    pub bytes_on_disk: u64,
}

/// Errors a sink can surface.
///
/// `AppendFailed` and `ReadFailed` are the only two the agent
/// runtime needs to distinguish for fail-fast behavior. The other
/// variants cover protocol-level problems (closed session,
/// unknown id) that callers may want to handle differently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionError {
    /// The sink refused an append because the session is already
    /// closed. Callers should restart with a new sink.
    Closed,
    /// Append failed (disk full, fsync error, network error on a
    /// remote backend). Caller MUST treat as fatal.
    AppendFailed(String),
    /// Read failed (corrupt file, IO error). Caller MAY retry.
    ReadFailed(String),
    /// Snapshot failed.
    SnapshotFailed(String),
    /// Close failed (best-effort — usually logged not raised).
    CloseFailed(String),
    /// The implementation rejected the request because of a
    /// backend-level invariant (e.g. quota, schema mismatch).
    Invalid(String),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => f.write_str("session is closed"),
            Self::AppendFailed(s)
            | Self::ReadFailed(s)
            | Self::SnapshotFailed(s)
            | Self::CloseFailed(s)
            | Self::Invalid(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for SessionError {}
