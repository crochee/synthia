//! Append-only event persistence for agent lifecycle events.
//!
//! Events are stored one-per-line in `{session_path}/events.jsonl` as
//! JSON-serialized [`PersistedEvent`] records. The store guarantees
//! monotonically increasing `seq` values within a session.

use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

const EVENTS_FILE: &str = "events.jsonl";

/// A single persisted event record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedEvent {
    /// Monotonic sequence number, unique within a session.
    pub seq: u64,
    /// Aggregate identifier (typically the session id).
    pub aggregate: String,
    /// Event type name.
    #[serde(rename = "type")]
    pub event_type: String,
    /// UTC timestamp when the event was persisted.
    pub ts: DateTime<Utc>,
    /// Source that produced the event.
    pub source: EventSource,
    /// Whether this event is ephemeral (observable but non-state-changing).
    ///
    /// Ephemeral events can be skipped during replay without affecting the
    /// projected `LoopContext` or `TurnTask` state. Defaults to `false`
    /// (durable) for backward compatibility with old JSONL files.
    #[serde(default)]
    pub ephemeral: bool,
    /// Event payload.
    pub payload: serde_json::Value,
}

/// The origin of a persisted event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventSource {
    /// Event emitted by the agent.
    Agent,
    /// Event emitted by the user.
    User,
    /// Event emitted by the system.
    System,
}

/// Append-only event store backed by `events.jsonl`.
///
/// Holds an in-process cache of the last allocated `seq` per session path,
/// so that `append` is O(1) after the first call per session (instead of
/// O(n) file scan on every append). The cache is shared across all clones
/// of the owning `Store` via `Arc<EventStore>`. The cache is lost on
/// process restart, in which case the next `append` re-scans the file to
/// find the true `max_seq`.
#[derive(Clone)]
pub struct EventStore {
    last_seq_cache: Arc<DashMap<PathBuf, AtomicU64>>,
}

impl EventStore {
    /// Create a new `EventStore` with an empty seq cache.
    pub fn new() -> Self {
        Self {
            last_seq_cache: Arc::new(DashMap::new()),
        }
    }

    /// Allocate the next seq for `session_path`, using the in-process cache
    /// when available, or scanning the file on first access (or after cache
    /// loss).
    ///
    /// Thread-safe: uses double-checked locking so that concurrent cache
    /// misses on the same session path serialize on the file scan rather
    /// than all returning the same `next` value.
    fn get_or_init_seq(&self, session_path: &Path) -> Result<u64> {
        // Fast path: read lock + atomic fetch_add, no file scan.
        if let Some(entry) = self.last_seq_cache.get(session_path) {
            return Ok(entry.fetch_add(1, Ordering::Relaxed));
        }
        // Slow path: acquire a write lock via entry() so initialization
        // is atomic. If another thread inserted the entry while we were
        // waiting for the write lock, we see Occupied and just bump the
        // counter instead of rescanning.
        use dashmap::mapref::entry::Entry;
        match self.last_seq_cache.entry(session_path.to_path_buf()) {
            Entry::Occupied(entry) => {
                Ok(entry.get().fetch_add(1, Ordering::Relaxed))
            }
            Entry::Vacant(entry) => {
                let max = max_seq(session_path)?;
                let next = max + 1;
                entry.insert(AtomicU64::new(next + 1));
                Ok(next)
            }
        }
    }

    /// Append a single event to `{session_path}/events.jsonl`.
    ///
    /// The sequence number is monotonically increasing, starting at 1
    /// for a new or legacy session that has no event log yet. The
    /// directory is created if it does not exist. O(1) after the first
    /// call per session (uses an in-process seq cache).
    pub fn append(
        &self,
        session_path: &Path,
        aggregate: &str,
        event_type: &str,
        source: EventSource,
        ephemeral: bool,
        payload: &serde_json::Value,
    ) -> Result<PersistedEvent> {
        fs::create_dir_all(session_path).with_context(|| {
            format!("Failed to create session directory: {:?}", session_path)
        })?;

        let seq = self.get_or_init_seq(session_path)?;
        let event = PersistedEvent {
            seq,
            aggregate: aggregate.to_string(),
            event_type: event_type.to_string(),
            ts: Utc::now(),
            source,
            ephemeral,
            payload: payload.clone(),
        };

        let path = session_path.join(EVENTS_FILE);
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| {
                format!("Failed to open events file: {:?}", path)
            })?;

        let line = serde_json::to_string(&event)?;
        writeln!(file, "{}", line)
            .with_context(|| format!("Failed to write event to: {:?}", path))?;
        file.sync_all().with_context(|| {
            format!("Failed to sync events file: {:?}", path)
        })?;

        Ok(event)
    }

    /// Read events with `seq > last_seq`, up to `limit` records.
    ///
    /// Returns events in chronological order. If `events.jsonl` does
    /// not exist (e.g. legacy session), an empty vector is returned.
    /// Reads from disk; does not consult the seq cache (crash-safe).
    pub fn read_from(
        &self,
        session_path: &Path,
        last_seq: u64,
        limit: usize,
    ) -> Result<Vec<PersistedEvent>> {
        let path = session_path.join(EVENTS_FILE);
        if !path.exists() {
            return Ok(Vec::new());
        }

        let file = fs::File::open(&path).with_context(|| {
            format!("Failed to open events file: {:?}", path)
        })?;
        let reader = BufReader::new(file);

        let mut events = Vec::new();
        for line in reader
            .lines()
            .map_while(Result::ok)
            .filter(|l| !l.trim().is_empty())
        {
            let event: PersistedEvent = serde_json::from_str(&line)
                .with_context(|| {
                    format!("Failed to deserialize event from: {:?}", path)
                })?;
            if event.seq > last_seq {
                events.push(event);
                if events.len() >= limit {
                    break;
                }
            }
        }

        Ok(events)
    }
}

impl Default for EventStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Return the highest sequence number currently stored in
/// `{session_path}/events.jsonl`.
///
/// Returns 0 if the file does not exist or is empty, which is the
/// starting point for a new or legacy session.
fn max_seq(session_path: &Path) -> Result<u64> {
    let path = session_path.join(EVENTS_FILE);
    if !path.exists() {
        return Ok(0);
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read events file: {:?}", path))?;
    let last_line = content.lines().rfind(|l| !l.trim().is_empty());

    match last_line {
        Some(line) => {
            let event: PersistedEvent = serde_json::from_str(line)
                .with_context(|| {
                    format!("Failed to deserialize last event from: {:?}", path)
                })?;
            Ok(event.seq)
        }
        None => Ok(0),
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn temp_session_path() -> (TempDir, std::path::PathBuf) {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("session");
        (temp, path)
    }

    #[test]
    fn test_append_creates_event_with_seq_one() {
        let (_temp, path) = temp_session_path();
        let store = EventStore::new();
        let event = store
            .append(
                &path,
                "session-1",
                "SessionStarted",
                EventSource::System,
                false,
                &serde_json::json!({"session_id": "session-1"}),
            )
            .unwrap();

        assert_eq!(event.seq, 1);
        assert_eq!(event.aggregate, "session-1");
        assert_eq!(event.event_type, "SessionStarted");
        assert_eq!(event.source, EventSource::System);
        assert!(path.join(EVENTS_FILE).exists());
    }

    #[test]
    fn test_read_from_returns_events_after_last_seq() {
        let (_temp, path) = temp_session_path();
        let store = EventStore::new();
        for i in 0..5 {
            store
                .append(
                    &path,
                    "session-1",
                    "IterationStarted",
                    EventSource::Agent,
                    false,
                    &serde_json::json!({"iteration": i}),
                )
                .unwrap();
        }

        let events = store.read_from(&path, 2, 10).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].seq, 3);
        assert_eq!(events[1].seq, 4);
        assert_eq!(events[2].seq, 5);
    }

    #[test]
    fn test_read_from_honors_limit() {
        let (_temp, path) = temp_session_path();
        let store = EventStore::new();
        for i in 0..5 {
            store
                .append(
                    &path,
                    "session-1",
                    "Progress",
                    EventSource::Agent,
                    false,
                    &serde_json::json!({"step": i}),
                )
                .unwrap();
        }

        let events = store.read_from(&path, 0, 2).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].seq, 1);
        assert_eq!(events[1].seq, 2);
    }

    #[test]
    fn test_read_from_missing_file_starts_at_one() {
        let (_temp, path) = temp_session_path();
        let store = EventStore::new();
        // events.jsonl does not exist yet
        let events = store.read_from(&path, 0, 10).unwrap();
        assert!(events.is_empty());

        let event = store
            .append(
                &path,
                "session-1",
                "SessionStarted",
                EventSource::System,
                false,
                &serde_json::json!({}),
            )
            .unwrap();
        assert_eq!(event.seq, 1);
    }

    #[test]
    fn test_seq_monotonicity_across_appends() {
        let (_temp, path) = temp_session_path();
        let store = EventStore::new();
        let mut prev_seq = 0u64;
        for i in 0..10 {
            let event = store
                .append(
                    &path,
                    "session-1",
                    "LlmStreamDelta",
                    EventSource::Agent,
                    false,
                    &serde_json::json!({"index": i}),
                )
                .unwrap();
            assert!(event.seq > prev_seq, "seq must increase monotonically");
            prev_seq = event.seq;
        }

        let events = store.read_from(&path, 0, usize::MAX).unwrap();
        assert_eq!(events.len(), 10);
        for (i, event) in events.iter().enumerate() {
            assert_eq!(event.seq, (i + 1) as u64);
        }
    }

    #[test]
    fn test_event_source_string_serialization() {
        assert_eq!(
            serde_json::to_string(&EventSource::Agent).unwrap(),
            "\"agent\""
        );
        assert_eq!(
            serde_json::to_string(&EventSource::User).unwrap(),
            "\"user\""
        );
        assert_eq!(
            serde_json::to_string(&EventSource::System).unwrap(),
            "\"system\""
        );
    }

    #[test]
    fn test_persisted_event_without_ephemeral_defaults_to_durable() {
        let old_format = r#"{"seq":1,"aggregate":"s","type":"SessionStarted","ts":"2025-01-01T00:00:00Z","source":"agent","payload":{}}"#;
        let event: PersistedEvent = serde_json::from_str(old_format).unwrap();
        assert!(!event.ephemeral);
    }

    #[test]
    fn test_seq_cache_avoids_rescan_on_subsequent_append() {
        let (_temp, path) = temp_session_path();
        let store = EventStore::new();

        // First append must scan the (empty) file to find max_seq = 0.
        let e1 = store
            .append(
                &path,
                "s",
                "Started",
                EventSource::System,
                false,
                &serde_json::json!({}),
            )
            .unwrap();
        assert_eq!(e1.seq, 1);

        // Second append should use the in-process cache, not rescan.
        // We verify by truncating the file AFTER the first append but BEFORE
        // the second: if append rescanned, it would see max_seq = 0 and emit
        // seq = 1 again (collision). With the cache, it emits seq = 2.
        let events_path = path.join(EVENTS_FILE);
        std::fs::write(&events_path, "").unwrap();

        let e2 = store
            .append(
                &path,
                "s",
                "Iter",
                EventSource::Agent,
                false,
                &serde_json::json!({}),
            )
            .unwrap();
        assert_eq!(e2.seq, 2, "cache must allocate seq=2 without rescanning");
    }

    #[test]
    fn test_shared_event_store_caches_across_multiple_appends() {
        let (_temp, path) = temp_session_path();
        let store = EventStore::new();

        // First append: scans file (cache miss).
        let e1 = store
            .append(
                &path,
                "s",
                "E",
                EventSource::Agent,
                false,
                &serde_json::json!({}),
            )
            .unwrap();
        assert_eq!(e1.seq, 1);

        // Truncate the file — if the next append rescans, it would see
        // max_seq = 0 and emit seq = 1 again (collision). With the cache,
        // it emits seq = 2.
        let events_path = path.join(EVENTS_FILE);
        std::fs::write(&events_path, "").unwrap();

        // Second append: uses cache (no rescan).
        let e2 = store
            .append(
                &path,
                "s",
                "E",
                EventSource::Agent,
                false,
                &serde_json::json!({}),
            )
            .unwrap();
        assert_eq!(e2.seq, 2);

        // Third append: still uses cache — verifies the cache increments
        // across multiple subsequent appends (the production pattern after
        // hoisting `EventStore` into `Store`).
        let e3 = store
            .append(
                &path,
                "s",
                "E",
                EventSource::Agent,
                false,
                &serde_json::json!({}),
            )
            .unwrap();
        assert_eq!(e3.seq, 3);
    }

    #[test]
    fn test_concurrent_appends_produce_unique_seqs() {
        let (_temp, path) = temp_session_path();
        let store = std::sync::Arc::new(EventStore::new());

        let mut handles = Vec::new();
        for _ in 0..10 {
            let store = store.clone();
            let path = path.clone();
            handles.push(std::thread::spawn(move || {
                store
                    .append(
                        &path,
                        "s",
                        "Concurrent",
                        EventSource::Agent,
                        false,
                        &serde_json::json!({}),
                    )
                    .unwrap()
                    .seq
            }));
        }
        let mut seqs: Vec<u64> =
            handles.into_iter().map(|h| h.join().unwrap()).collect();
        seqs.sort_unstable();
        seqs.dedup();
        assert_eq!(seqs.len(), 10, "all seqs must be unique");
        assert_eq!(seqs[0], 1);
        assert_eq!(seqs[9], 10);
    }

    #[test]
    fn test_crash_recovery_rescans_for_max_seq() {
        let (_temp, path) = temp_session_path();
        let store = EventStore::new();

        // Append 3 events.
        for _ in 0..3 {
            store
                .append(
                    &path,
                    "s",
                    "E",
                    EventSource::Agent,
                    false,
                    &serde_json::json!({}),
                )
                .unwrap();
        }

        // Simulate process restart: drop store, create new one (cache empty).
        let store = EventStore::new();
        let e = store
            .append(
                &path,
                "s",
                "E",
                EventSource::Agent,
                false,
                &serde_json::json!({}),
            )
            .unwrap();
        assert_eq!(e.seq, 4, "after restart, must rescan and find max_seq=3");
    }
}
