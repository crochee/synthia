//! 3-layer memory tier for the agent runtime.
//!
//! Ported from `traitclaw-core`'s `Memory` trait: one trait,
//! three tiers, and session lifecycle helpers with no-op
//! defaults so partial backends stay cheap to implement.
//!
//! # Tiers
//!
//! | Tier | Methods | Lifetime | Backing in [`SessionMemory`] |
//! |---|---|---|---|
//! | **Conversation** — short-term message history | [`messages`](Memory::messages) / [`append`](Memory::append) | durable | `SessionSink` JSONL log |
//! | **Working** — task-scoped key-value scratch state | [`get_context`](Memory::get_context) / [`set_context`](Memory::set_context) | process | in-proc map |
//! | **Long-term** — semantic recall across sessions | [`recall`](Memory::recall) / [`store`](Memory::store) | durable (survives restarts) | in-proc vec, or scoped files via [`FileMemory`] |
//!
//! # Layering
//!
//! ```text
//! synthia-server ──▶ synthia-harness ──▶ synthia-context ──▶ synthia-provider
//!                                          │
//!                                          ▼ (conversation tier only)
//!                                   synthia-session (SessionSink)
//! ```
//!
//! The `synthia-session` crate stays an inert leaf crate: this module is
//! the only place that bridges the provider wire types
//! ([`Message`]) and the opaque sink event log, via the
//! [`events_to_messages`] projection. The live context window
//! remains the [`crate::ContextManager`]'s job — memory tiers
//! answer "what happened / what do we know", the context manager
//! answers "what fits in the next prompt".
//!
//! # Session lifecycle defaults
//!
//! [`create_session`](Memory::create_session) mints a `ULID`;
//! [`list_sessions`](Memory::list_sessions) and
//! [`delete_session`](Memory::delete_session) default to no-ops
//! for backends without directory semantics. Long-term memory is
//! global across sessions and intentionally NOT cleared by
//! `delete_session`.

mod file;
mod in_memory;
mod session;
#[cfg(feature = "sqlite")]
mod sqlite;
mod typed;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
pub use file::{
    FileMemory,
    FileMemoryConfig,
    FileMemoryEntry,
    FileMemoryError,
    MAX_MEMORY_INDEX_LINES,
    MemoryKind,
    MemoryScope,
};
pub use in_memory::InMemoryMemory;
use serde_json::Value;
pub use session::{SessionMemory, events_to_messages};
#[cfg(feature = "sqlite")]
pub use sqlite::{SqliteMemory, SqliteMemoryError};
use synthia_core::Clock;
use synthia_provider::Message;
use synthia_session::SessionError;
pub use typed::{
    TypedProjectionError,
    project as project_typed_messages,
    repair_session,
    typed_messages_from_sink,
};
/// A stored memory entry for long-term recall.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryEntry {
    /// Unique identifier for this entry.
    pub id: String,
    /// The content of the memory.
    pub content: String,
    /// Optional metadata associated with this entry.
    pub metadata: Option<Value>,
    /// Wall-clock time when this entry was created (UTC).
    /// `chrono` is the workspace-wide time library, so the
    /// entry carries a typed timestamp instead of a raw epoch
    /// count and serialises as RFC 3339.
    pub created_at: DateTime<Utc>,
}

impl MemoryEntry {
    /// Create a new `MemoryEntry` with `created_at` set to the
    /// current UTC time (through
    /// [`synthia_core::Clock`], not
    /// `chrono::Utc::now`).
    #[must_use]
    pub fn now(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::at(id, content, synthia_core::SharedClock::system().now())
    }

    /// [`MemoryEntry::now`] with an explicit timestamp — for tests and
    /// for callers whose clock comes from elsewhere.
    #[must_use]
    pub fn at(
        id: impl Into<String>,
        content: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: id.into(),
            content: content.into(),
            metadata: None,
            created_at,
        }
    }
}

/// Errors the memory tier can surface.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MemoryError {
    /// The backing session sink failed. Fatal at the call site —
    /// the run boundary owns retries, not the memory tier.
    #[error("session sink failure: {0}")]
    Sink(#[from] SessionError),
    /// A stored event envelope could not be decoded. The
    /// projection drops such rows (see [`events_to_messages`]);
    /// this variant covers the encode/decode paths that must
    /// propagate (e.g. `append` of a non-serialisable payload).
    #[error("event envelope decode failure: {0}")]
    Decode(String),
    /// The file-backed long-term tier failed: a refused path, a
    /// refused write, or a filesystem error. See
    /// [`FileMemoryError`] for the typed cause.
    #[error(transparent)]
    File(#[from] FileMemoryError),
    /// The SQLite-backed tier failed: a refused open, a failed
    /// migration, or a query error. See [`SqliteMemoryError`] for the
    /// typed cause.
    #[cfg(feature = "sqlite")]
    #[error(transparent)]
    Sqlite(#[from] SqliteMemoryError),
}

/// Trait for the 3-layer memory system.
///
/// Provides conversation history, working memory, and long-term
/// recall behind one seam, so the agent runtime and the server
/// rehydration path share a single typed view instead of each
/// parsing the durable event log ad-hoc.
#[async_trait]
pub trait Memory: Send + Sync {
    // === Conversation memory (short-term) ===

    /// Get conversation messages for a session, in chronological
    /// order.
    async fn messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<Message>, MemoryError>;

    /// Append a message to the conversation history.
    async fn append(
        &self,
        session_id: &str,
        message: Message,
    ) -> Result<(), MemoryError>;

    // === Working memory (task-scoped) ===

    /// Get a value from working memory.
    async fn get_context(
        &self,
        session_id: &str,
        key: &str,
    ) -> Result<Option<Value>, MemoryError>;

    /// Set a value in working memory.
    async fn set_context(
        &self,
        session_id: &str,
        key: &str,
        value: Value,
    ) -> Result<(), MemoryError>;

    // === Long-term memory (semantic recall) ===

    /// Search for relevant memories. An empty `query` matches all
    /// entries; results are truncated to `limit` and callers
    /// should treat a full result set as potentially truncated.
    async fn recall(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>, MemoryError>;

    /// Store a new memory entry.
    async fn store(&self, entry: MemoryEntry) -> Result<(), MemoryError>;

    // === Session lifecycle (defaults; override for directory
    // or persistent backends) ===

    /// Create a new session and return its ID. Default mints a
    /// `ULID`.
    async fn create_session(&self) -> Result<String, MemoryError> {
        Ok(ulid::Ulid::generate().to_string())
    }

    /// List all known session IDs. Default returns an empty vec.
    async fn list_sessions(&self) -> Result<Vec<String>, MemoryError> {
        Ok(Vec::new())
    }

    /// Delete a session's conversation history and working
    /// memory. Long-term memory is global and intentionally NOT
    /// cleared. Default is a no-op.
    async fn delete_session(
        &self,
        _session_id: &str,
    ) -> Result<(), MemoryError> {
        Ok(())
    }
}
