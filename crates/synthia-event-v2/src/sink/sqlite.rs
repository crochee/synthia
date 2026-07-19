//! SQLite-backed [`EventBus`](crate::EventBus) implementation with dual-table
//! persistence (PR-1.4).
//!
//! Implementation choices (see `design.md` §D1):
//!
//! - **Storage**: `rusqlite` 0.32+ with two tables — `events` (raw payloads)
//!   and `projections` (materialised views built by `Projector` impls).
//! - **Schema**: auto-created on construction via an idempotent `CREATE TABLE
//!   IF NOT EXISTS` migration; no external migration tool required.
//! - **Thread safety**: `rusqlite::Connection` is `!Sync`; the sink wraps it
//!   in a `parking_lot::Mutex` so `EventBus::emit` can be called from any
//!   tokio task.
//! - **Restart recovery**: because events live in SQLite, a process restart
//!   replays the event history via `aggregate_events::<T>()` without loss.
//!   The in-memory sink drops events on process exit by design.
//!
//! The `sqlite` Cargo feature must be enabled to compile this module.

#![cfg(feature = "sqlite")]

use std::{collections::HashMap, path::Path};

use parking_lot::Mutex;
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::{
    EmitOutcome,
    EventBus,
    EventBusError,
    EventSinkKind,
    event::{
        EventEnvelope,
        EventMeta,
        EventSource,
        EventVersion,
        SourceSequence,
    },
};

/// Default SQLite file path (in-memory for testing; real path in production).
const DEFAULT_PATH: &str = "synthia_event_v2.db";

/// SQL schema for the `events` table.
const CREATE_EVENTS_TABLE: &str = "\
CREATE TABLE IF NOT EXISTS events (\
    envelope_id TEXT PRIMARY KEY, \
    source      TEXT NOT NULL, \
    sequence    INTEGER NOT NULL, \
    prefix_hash TEXT NOT NULL, \
    created_at_ms INTEGER NOT NULL, \
    payload     TEXT NOT NULL, \
    UNIQUE(source, sequence)\
)";

/// SQL schema for the `projections` table.
const CREATE_PROJECTIONS_TABLE: &str = "\
CREATE TABLE IF NOT EXISTS projections (\
    id          INTEGER PRIMARY KEY AUTOINCREMENT, \
    envelope_id TEXT NOT NULL REFERENCES events(envelope_id) ON DELETE CASCADE, \
    projector   TEXT NOT NULL, \
    result      TEXT NOT NULL, \
    created_at_ms INTEGER NOT NULL\
)";

/// Index on `events.created_at_ms` for the 7-day retention cleanup query.
const CREATE_EVENTS_CREATED_AT_INDEX: &str = "\
CREATE INDEX IF NOT EXISTS idx_events_created_at_ms ON events(created_at_ms)\
";

/// Index on `events.source` for per-source replay.
const CREATE_EVENTS_SOURCE_INDEX: &str = "\
CREATE INDEX IF NOT EXISTS idx_events_source ON events(source)\
";

/// SQLite-backed durable event bus (PR-1.4).
///
/// Persists every emitted event into the `events` table. Downstream
/// `Projector` impls write their outputs into `projections`. Both tables
/// survive process restart.
pub struct SqliteEventBus {
    /// SQLite connection (wrapped in Mutex for Sync).
    conn: Mutex<Connection>,
    /// Per-source monotonic sequence allocator (reloaded from DB on open).
    sequences: Mutex<HashMap<EventSource, SourceSequence>>,
    /// Set to `true` once `is_closed` has been set.
    closed: Mutex<bool>,
}

impl std::fmt::Debug for SqliteEventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteEventBus")
            .field("closed", &*self.closed.lock())
            .finish_non_exhaustive()
    }
}

impl SqliteEventBus {
    /// Open (or create) a SQLite event bus at `path`.
    ///
    /// On success the dual-table schema is guaranteed to exist. If the file
    /// already contains events, per-source sequences are restored so
    /// subsequent `emit` calls continue monotonically.
    pub fn open(path: &Path) -> Result<Self, EventBusError> {
        let conn = Connection::open(path)
            .map_err(|e| EventBusError::Sink(e.to_string()))?;

        Self::init_schema(&conn)?;

        // Restore sequences from existing data so restarts continue
        // monotonically.
        let sequences = Self::restore_sequences(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
            sequences: Mutex::new(sequences),
            closed: Mutex::new(false),
        })
    }

    /// Open an in-memory SQLite bus (useful for testing).
    pub fn open_in_memory() -> Result<Self, EventBusError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| EventBusError::Sink(e.to_string()))?;

        Self::init_schema(&conn)?;

        let sequences = Self::restore_sequences(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
            sequences: Mutex::new(sequences),
            closed: Mutex::new(false),
        })
    }

    /// Open at the default path (`synthia_event_v2.db` in cwd).
    pub fn open_default() -> Result<Self, EventBusError> {
        Self::open(Path::new(DEFAULT_PATH))
    }

    /// Create the dual-table schema and indexes.
    fn init_schema(conn: &Connection) -> Result<(), EventBusError> {
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .map_err(|e| EventBusError::Sink(e.to_string()))?;

        conn.execute(CREATE_EVENTS_TABLE, [])
            .map_err(|e| EventBusError::Sink(e.to_string()))?;
        conn.execute(CREATE_PROJECTIONS_TABLE, [])
            .map_err(|e| EventBusError::Sink(e.to_string()))?;
        conn.execute(CREATE_EVENTS_CREATED_AT_INDEX, [])
            .map_err(|e| EventBusError::Sink(e.to_string()))?;
        conn.execute(CREATE_EVENTS_SOURCE_INDEX, [])
            .map_err(|e| EventBusError::Sink(e.to_string()))?;

        Ok(())
    }

    /// Restore per-source sequences from existing events so that after a
    /// process restart, `emit` continues with the next sequence number.
    fn restore_sequences(
        conn: &Connection,
    ) -> Result<HashMap<EventSource, SourceSequence>, EventBusError> {
        let mut sequences = HashMap::new();
        let mut stmt = conn
            .prepare("SELECT source, MAX(sequence) FROM events GROUP BY source")
            .map_err(|e| EventBusError::Sink(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                let source_str: String = row.get(0)?;
                let max_seq: u64 = row.get(1)?;
                Ok((source_str, max_seq))
            })
            .map_err(|e| EventBusError::Sink(e.to_string()))?;

        for row in rows {
            let (source_str, max_seq) =
                row.map_err(|e| EventBusError::Sink(e.to_string()))?;
            let source = parse_event_source(&source_str);
            let seq = SourceSequence::starting_at(max_seq);
            sequences.insert(source, seq);
        }

        Ok(sequences)
    }

    /// Delete events older than `retention_ms` milliseconds.
    ///
    /// Used by the `CleanupTask` (7-day retention). Returns the number of
    /// deleted event rows.
    pub fn cleanup_old_events(
        &self,
        retention_ms: i64,
    ) -> Result<usize, EventBusError> {
        let now_ms = now_ms();
        let cutoff = now_ms - retention_ms;

        let conn = self.conn.lock();

        // Delete projections referencing events that will be removed.
        conn.execute(
            "DELETE FROM projections WHERE envelope_id IN \
             (SELECT envelope_id FROM events WHERE created_at_ms < ?1)",
            params![cutoff],
        )
        .map_err(|e| EventBusError::Sink(e.to_string()))?;

        let deleted = conn
            .execute(
                "DELETE FROM events WHERE created_at_ms < ?1",
                params![cutoff],
            )
            .map_err(|e| EventBusError::Sink(e.to_string()))?;

        Ok(deleted)
    }

    /// Count total events in the `events` table.
    #[cfg(test)]
    pub fn event_count(&self) -> Result<usize, EventBusError> {
        let conn = self.conn.lock();
        let count: usize = conn
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
            .map_err(|e| EventBusError::Sink(e.to_string()))?;
        Ok(count)
    }
}

#[async_trait::async_trait]
impl EventBus for SqliteEventBus {
    fn kind(&self) -> EventSinkKind {
        EventSinkKind::Sqlite
    }

    async fn emit<P>(&self, payload: &P) -> Result<EmitOutcome, EventBusError>
    where
        P: Serialize + Sync + Send + 'static,
    {
        if *self.closed.lock() {
            return Err(EventBusError::Closed);
        }

        let payload_json = serde_json::to_value(payload)?;

        // Per-source monotonic sequence allocation.
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
        let envelope: EventEnvelope<serde_json::Value> =
            EventEnvelope::new(version, meta.clone(), payload_json.clone());

        let envelope_id = envelope.envelope_id.to_string();
        let source_str = source.as_str();
        let prefix_hash_hex = meta.prefix_hash.hex();

        // Persist to SQLite.
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO events (envelope_id, source, sequence, prefix_hash, created_at_ms, payload) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                envelope_id,
                source_str,
                sequence,
                prefix_hash_hex,
                meta.created_at_ms,
                serde_json::to_string(&payload_json)
                    .map_err(EventBusError::Serialize)?,
            ],
        )
        .map_err(|e| EventBusError::Sink(e.to_string()))?;

        Ok(EmitOutcome::Persisted)
    }

    fn is_closed(&self) -> bool {
        *self.closed.lock()
    }
}

impl Drop for SqliteEventBus {
    fn drop(&mut self) {
        *self.closed.lock() = true;
        tracing::trace!(
            target: "synthia::event_v2",
            sink = "sqlite",
            "sqlite event bus dropped; data persisted to disk",
        );
    }
}

/// Parse an `EventSource` from its string representation.
fn parse_event_source(s: &str) -> EventSource {
    match s {
        "agent" => EventSource::Agent,
        "tool" => EventSource::Tool,
        "user" => EventSource::User,
        _ => EventSource::System,
    }
}

/// Wall-clock time in milliseconds since the Unix epoch.
fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_creates_schema() {
        let bus = SqliteEventBus::open_in_memory().unwrap();
        assert!(!bus.is_closed());
        assert_eq!(bus.kind(), EventSinkKind::Sqlite);
        assert_eq!(bus.event_count().unwrap(), 0);
    }

    #[test]
    fn emit_persists_to_sqlite() {
        let bus = SqliteEventBus::open_in_memory().unwrap();
        let outcome = tokio_run(bus.emit(&serde_json::json!({"test": 1})));
        assert!(matches!(outcome, Ok(EmitOutcome::Persisted)));
        assert_eq!(bus.event_count().unwrap(), 1);
    }

    #[test]
    fn emit_increments_sequence() {
        let bus = SqliteEventBus::open_in_memory().unwrap();
        let _ = tokio_run(bus.emit(&"a"));
        let _ = tokio_run(bus.emit(&"b"));
        let _ = tokio_run(bus.emit(&"c"));
        assert_eq!(bus.event_count().unwrap(), 3);
    }

    #[test]
    fn cleanup_deletes_old_events() {
        let bus = SqliteEventBus::open_in_memory().unwrap();
        let _ = tokio_run(bus.emit(&"old"));
        let _ = tokio_run(bus.emit(&"new"));

        // Use negative retention_ms so cutoff = now - (-1) = now + 1,
        // which is guaranteed to be newer than any stored event.
        let deleted = bus.cleanup_old_events(-1).unwrap();
        assert!(deleted >= 2);
        assert_eq!(bus.event_count().unwrap(), 0);
    }

    #[test]
    fn closed_bus_rejects_emit() {
        let bus = SqliteEventBus::open_in_memory().unwrap();
        *bus.closed.lock() = true;
        let result = tokio_run(bus.emit(&"test"));
        assert!(matches!(result, Err(EventBusError::Closed)));
    }

    fn tokio_run<T>(fut: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(fut)
    }
}
