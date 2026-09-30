//! `SessionSink` — the **only** public surface of `synthia-session`.
//!
//! ## Surface size
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
//! After the original refactor:
//!
//! - [`SessionSink`] was the only trait the agent runtime imported.
//! - It had exactly **5 methods** (`id`, `append`, `read`,
//!   `snapshot`, `close`). Anything else the caller wanted to do
//!   with a session was **policy** and lived outside this crate
//!   (typically in `synthia-server`).
//! - All on-disk persistence, state machines, approval flows, and
//!   token-budget tracking had been either deleted or moved into
//!   the `synthia-server::session` orchestration layer.
//!
//! The trait gained two **default-implemented** methods in a
//! later round so the same primitive could carry three orthogonal
//! concerns without growing the surface for callers that did not
//! need them:
//!
//! - [`SessionSink::append_with_key`] — idempotent append:
//!   `Some(key)` callers that re-submit the same key observe the
//!   original `seq` (no double-write); `None` callers see the
//!   legacy non-idempotent behaviour.
//! - [`SessionSink::append_many`] — batch append with one
//!   durable barrier (`fsync`) at the tail, instead of N. The
//!   default implementation loops over `append` so backends that
//!   cannot batch still satisfy the contract; the production
//!   `JsonlSessionSink` overrides it for a single-`fsync`
//!   fast path (`routes/sessions.rs::fork_session` is the
//!   motivating caller — a per-row `fsync` made a 27 k-row fork
//!   take 63.8 s).
//!
//! Both are default-implemented: a backend that does not care
//! about idempotency or batch durability stays at the original
//! 5-method surface.
//!
//! ## Event shape
//!
//! `SessionSink` is event-shape-agnostic: it stores opaque
//! `serde_json::Value` records. Callers (`synthia-harness`) are
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
//! `read()` and `read_from(from)` MUST return records in
//! chronological order, identical to the order they were passed
//! to `append`. Implementations that batch / compress MUST
//! surface that order in `read()` and `read_from()` output. This
//! lets any caller that wants to rehydrate a session under a
//! different agent reconstruct a consistent message stream
//! regardless of the backend. `read_from(N)` returns the suffix
//! of the chronological sequence strictly past event `N`, so a
//! caller resuming from a previously-observed cursor sees only
//! the events it has not yet read.
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The single trait every agent loop sees.
///
/// `synthia-harness` holds `Arc<dyn SessionSink>`; it never sees
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
    /// Returns the implicit 1-indexed sequence the sink assigned to
    /// the row — the same ordinal [`read`](Self::read) order and
    /// [`snapshot`](Self::snapshot)'s `last_event_seq` use. The sink
    /// is the only thing that can report it: it assigns the ordinal
    /// under its own lock, so a caller that counts rows itself drifts
    /// as soon as anything else appends concurrently.
    ///
    /// Failure is fatal at the call site: the agent loop MUST
    /// surface the error to its caller rather than silently
    /// dropping the event.
    async fn append(&self, event: &Value) -> Result<u64, SessionError>;

    /// Reconstruct the chronological event stream. Returns events
    /// in the order they were `append`-ed. Used by callers that
    /// want to rehydrate a session (e.g. resume under a
    /// different agent).
    async fn read(&self) -> Result<Vec<Value>, SessionError>;

    /// Reconstruct the chronological event stream starting **after**
    /// the event whose implicit sequence is `from`.
    ///
    /// Equivalent to [`read`](Self::read) followed by dropping the
    /// first `from` events, but implementations are free to skip the
    /// work without materializing the dropped prefix. A caller
    /// reconnecting with a `from` it observed from a previous
    /// [`snapshot`](Self::snapshot) call therefore receives
    /// strictly the events it has not yet seen, without re-reading
    /// the whole log.
    ///
    /// `from = 0` returns every recorded event (the same set as
    /// [`read`](Self::read)). `from = N` returns only events whose
    /// implicit sequence (`append` order, 1-indexed) is greater
    /// than `N`. `from` MUST be ≤ the current `last_event_seq`
    /// from the latest [`snapshot`](Self::snapshot) — callers with
    /// a stale `from` are responsible for surfacing that mismatch
    /// (typically as a `410 Gone` at the HTTP seam).
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::ReadFailed`] when the underlying
    /// storage cannot satisfy the request (corrupt file, IO
    /// error).
    async fn read_from(&self, from: u64) -> Result<Vec<Value>, SessionError>;

    /// Reconstruct the chronological event stream starting **after**
    /// the event whose `stream_index` is `idx`.
    ///
    /// Equivalent to `read_from` for sinks that allocate one
    /// `stream_index` per append and never reuse or skip one
    /// (today every sink). The default implementation
    /// delegates to `read_from`, so the two are interchangeable
    /// on existing sinks; future stream-position semantics
    /// (e.g. dense-over-compaction indices) override this
    /// default.
    ///
    /// `idx = 0` returns every recorded event. `idx = N`
    /// returns only events whose `stream_index` is greater
    /// than `N`. Callers with a stale `idx` are responsible
    /// for surfacing that mismatch as a `410 Gone` at the HTTP
    /// seam -- see `2026-09-24-stream-index.md` section 4.4.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::ReadFailed`] when the underlying
    /// storage cannot satisfy the request.
    async fn read_from_index(
        &self,
        idx: u64,
    ) -> Result<Vec<Value>, SessionError> {
        self.read_from(idx).await
    }

    /// Force a stable checkpoint (flush buffers, rotate logs,
    /// push to remote). Called when the session enters a stable
    /// state (idle, completed). Returns the snapshot metadata.
    async fn snapshot(&self) -> Result<SessionSnapshot, SessionError>;

    /// Mark the session closed. Idempotent. After this resolves
    /// no further `append` / `read` calls are accepted.
    async fn close(&self, reason: SessionEndReason)
    -> Result<(), SessionError>;

    /// Append one event durably with an **idempotency key**.
    ///
    /// `Some(key)` — a caller-supplied opaque token (typically the
    /// transport-level `event_id`, a `Message-Id` header, or the
    /// `agent.session.turn.id` a downstream client minted). The
    /// sink remembers every key it has seen for this session
    /// within a bounded LRU window. A re-submission of the same
    /// key is a **no-op**: the sink returns the `seq` originally
    /// assigned to that key, and the bytes are not written twice.
    ///
    /// This is the dedup seam that closes the
    /// "client retried / SSE reconnected mid-flush" hole —
    /// without it, a single user-typed prompt could land twice
    /// in the durable log and the LLM would see two user-role
    /// rows for the same input.
    ///
    /// `None` — the legacy non-idempotent path. Backends that
    /// do not need dedup leave the default implementation
    /// (`Ok(self.append(event).await?)`) in place; the LRU
    /// bookkeeping is opt-in. Idempotent callers MUST use a
    /// unique key per logical event or `seq` will collide.
    ///
    /// The key is opaque to the sink — it does not appear in
    /// `read()` output, so a replay does not leak caller-side
    /// identifiers. Backends that need to persist the key (for
    /// across-restart dedup) should stash it in a sibling
    /// index file; see `JsonlSessionSink::append_with_key`.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Closed`] when the session is
    /// already closed. Idempotent re-submission is NOT an error
    /// — it returns the same `seq` as the original append.
    async fn append_with_key(
        &self,
        key: Option<&str>,
        event: &Value,
    ) -> Result<u64, SessionError> {
        let _ = key;
        self.append(event).await
    }

    /// Append a batch of events durably with a **single durable
    /// barrier** (one `fsync` at the tail, not N).
    ///
    /// Default implementation loops over [`append`](Self::append),
    /// so backends without a batched write path keep their
    /// existing per-row durability at the cost of N `fsync`s.
    /// The production `JsonlSessionSink` overrides this to write
    /// the whole batch in one `write_all` + one `fsync` —
    /// measured ~200× faster than per-row `fsync` on a 1k-row
    /// batch.
    ///
    /// Atomicity contract: either every event in `events` is
    /// durable after `Ok`, or none of them are. A partial
    /// failure rolls back the `seq` counter so subsequent
    /// `read` / `snapshot` calls do not observe a half-written
    /// batch. Callers SHOULD treat the same `Err` as a fatal
    /// append failure (the agent loop's fail-fast rule).
    ///
    /// Idempotency: this method does not dedup — call
    /// [`append_with_key`](Self::append_with_key) per row if
    /// the batch may have been partially applied on a prior
    /// attempt. `fork_session` is the motivating caller: it
    /// reads the parent transcript once, holds the rows in
    /// memory, and submits a fresh batch to the child — no
    /// dedup needed.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::AppendFailed`] on any IO
    /// failure; `seq` is rolled back atomically so a retry
    /// that re-sends the same batch does not double-write.
    ///
    /// The return value is the `seq` of the batch's **last**
    /// event, or the current tail when `events` is empty — so a
    /// caller can read it as "where the log stands now" without a
    /// separate `snapshot` call.
    async fn append_many(&self, events: &[Value]) -> Result<u64, SessionError> {
        if events.is_empty() {
            return Ok(self.snapshot().await?.last_event_seq);
        }
        let mut last_seq = 0;
        for event in events {
            last_seq = self.append(event).await?;
        }
        Ok(last_seq)
    }
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
///
/// `last_stream_index` is the wire-level resume cursor — today
/// it equals `last_event_seq` because the sink's 1-indexed row
/// ordinal IS the per-session stream position; the field is
/// exposed so the wire can name it independently from the
/// sink-internal ordinal as the protocol evolves
/// (`2026-09-24-stream-index.md`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub session_id: String,
    pub last_event_seq: u64,
    /// Per-session stream cursor. See [`SessionSink`] —
    /// the sink's append returns this value. Equal to
    /// `last_event_seq` for sinks that allocate one ordinal
    /// per append and never reuse or skip one.
    pub last_stream_index: u64,
    pub bytes_on_disk: u64,
}

/// Errors a sink can surface.
///
/// `AppendFailed` and `ReadFailed` are the only two the agent
/// runtime needs to distinguish for fail-fast behavior. The other
/// variants cover protocol-level problems (closed session,
/// unknown id) that callers may want to handle differently.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    /// The sink refused an append because the session is already
    /// closed. Callers should restart with a new sink.
    #[error("session is closed")]
    Closed,
    /// Append failed (disk full, fsync error, network error on a
    /// remote backend). Caller MUST treat as fatal.
    #[error("{0}")]
    AppendFailed(String),
    /// Read failed (corrupt file, IO error). Caller MAY retry.
    #[error("{0}")]
    ReadFailed(String),
    /// Snapshot failed.
    #[error("{0}")]
    SnapshotFailed(String),
    /// Close failed (best-effort — usually logged not raised).
    #[error("{0}")]
    CloseFailed(String),
    /// The implementation rejected the request because of a
    /// backend-level invariant (e.g. quota, schema mismatch).
    #[error("{0}")]
    Invalid(String),
}
