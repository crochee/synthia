//! In-memory `SessionSink` for tests.
//!
//! Holds events in a `Vec` under a `Mutex`. NOT persisted; a
//! process restart loses all events. Callers (the agent test
//! suite) MUST treat this as ephemeral.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::Value;

use crate::sink::{
    SessionEndReason,
    SessionError,
    SessionSink,
    SessionSnapshot,
};

/// Append-only in-memory sink.
///
/// `InMemorySessionSink` is the canonical test backend: agent
/// loop tests can construct one without touching the filesystem,
/// and `read()` returns the same events that were `append`-ed.
///
/// Closed state is sticky: once `close()` returns `Ok`, every
/// subsequent `append` returns `Err(SessionError::Closed)`.
#[derive(Clone)]
pub struct InMemorySessionSink {
    id: String,
    state: Arc<Mutex<State>>,
}

struct State {
    events: Vec<Value>,
    closed: bool,
    seq: u64,
    /// Set of every idempotency key the sink has accepted for
    /// this session. Mirrors the JSONL backend's LRU without
    /// the bounded window — tests do not exercise the eviction
    /// path, and the in-memory backend is ephemeral anyway.
    /// Keys are owned `String`s so the trait API can hand us a
    /// borrowed `&str` and we store it cheaply.
    idem_keys: std::collections::HashSet<String>,
    /// Reverse index: idempotency key → assigned seq. Separate
    /// from `idem_keys` so a dup hit returns the seq directly
    /// without re-deriving it from the events vec.
    idem_seqs: std::collections::HashMap<String, u64>,
}

impl InMemorySessionSink {
    /// Create a new in-memory sink with the given session id.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            state: Arc::new(Mutex::new(State {
                events: Vec::new(),
                closed: false,
                seq: 0,
                idem_keys: std::collections::HashSet::new(),
                idem_seqs: std::collections::HashMap::new(),
            })),
        }
    }

    /// Number of events currently stored. Useful for test
    /// assertions.
    pub fn len(&self) -> usize {
        self.state
            .lock()
            .expect("InMemorySessionSink poisoned")
            .events
            .len()
    }

    /// Whether the sink has zero events.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl SessionSink for InMemorySessionSink {
    fn id(&self) -> &str {
        &self.id
    }

    async fn append(&self, event: &Value) -> Result<u64, SessionError> {
        let mut s = self.state.lock().expect("InMemorySessionSink poisoned");
        if s.closed {
            return Err(SessionError::Closed);
        }
        s.seq += 1;
        s.events.push(event.clone());
        Ok(s.seq)
    }

    async fn append_with_key(
        &self,
        key: Option<&str>,
        event: &Value,
    ) -> Result<u64, SessionError> {
        let Some(key) = key else {
            return self.append(event).await;
        };
        let mut s = self.state.lock().expect("InMemorySessionSink poisoned");
        if let Some(&seq) = s.idem_seqs.get(key) {
            return Ok(seq);
        }
        if s.closed {
            return Err(SessionError::Closed);
        }
        s.seq += 1;
        let seq = s.seq;
        s.events.push(event.clone());
        s.idem_keys.insert(key.to_string());
        s.idem_seqs.insert(key.to_string(), seq);
        Ok(seq)
    }

    async fn append_many(&self, events: &[Value]) -> Result<u64, SessionError> {
        let mut s = self.state.lock().expect("InMemorySessionSink poisoned");
        if s.closed {
            return Err(SessionError::Closed);
        }
        if events.is_empty() {
            return Ok(s.seq);
        }
        for event in events {
            s.events.push(event.clone());
            s.seq += 1;
        }
        Ok(s.seq)
    }

    async fn read(&self) -> Result<Vec<Value>, SessionError> {
        let s = self.state.lock().expect("InMemorySessionSink poisoned");
        Ok(s.events.clone())
    }

    async fn read_from(&self, from: u64) -> Result<Vec<Value>, SessionError> {
        let s = self.state.lock().expect("InMemorySessionSink poisoned");
        let skip = usize::try_from(from).unwrap_or(usize::MAX);
        Ok(s.events.iter().skip(skip).cloned().collect())
    }

    async fn snapshot(&self) -> Result<SessionSnapshot, SessionError> {
        let s = self.state.lock().expect("InMemorySessionSink poisoned");
        Ok(SessionSnapshot {
            session_id: self.id.clone(),
            last_event_seq: s.seq,
            last_stream_index: s.seq,
            bytes_on_disk: 0,
        })
    }

    async fn close(
        &self,
        _reason: SessionEndReason,
    ) -> Result<(), SessionError> {
        let mut s = self.state.lock().expect("InMemorySessionSink poisoned");
        s.closed = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn in_memory_sink_round_trips_events() {
        let s = InMemorySessionSink::new("test");
        s.append(&json!({"i": 0})).await.unwrap();
        s.append(&json!({"i": 1})).await.unwrap();
        let events = s.read().await.unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["i"], 0);
        assert_eq!(events[1]["i"], 1);
    }

    #[tokio::test]
    async fn in_memory_sink_rejects_appends_after_close() {
        let s = InMemorySessionSink::new("test");
        s.close(SessionEndReason::Completed).await.unwrap();
        let err = s.append(&json!({"i": 0})).await.unwrap_err();
        assert_eq!(err, SessionError::Closed);
    }

    #[tokio::test]
    async fn in_memory_sink_snapshot_returns_sequence() {
        let s = InMemorySessionSink::new("test");
        s.append(&json!({"i": 0})).await.unwrap();
        s.append(&json!({"i": 1})).await.unwrap();
        let snap = s.snapshot().await.unwrap();
        assert_eq!(snap.session_id, "test");
        assert_eq!(snap.last_event_seq, 2);
    }

    #[tokio::test]
    async fn in_memory_sink_close_is_idempotent() {
        let s = InMemorySessionSink::new("test");
        s.close(SessionEndReason::Completed).await.unwrap();
        s.close(SessionEndReason::Completed).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn in_memory_sink_concurrent_appends_preserve_all_records() {
        // Mirrors the JSONL concurrent-append test against the
        // in-memory backend. 50 concurrent appends across 4
        // worker threads must all land; the lock is the only
        // serialization point.
        let s = InMemorySessionSink::new("concurrent");
        let mut handles = Vec::new();
        for i in 0..50 {
            let sink = s.clone();
            handles.push(tokio::spawn(async move {
                sink.append(&json!({"i": i})).await
            }));
        }
        for h in handles {
            h.await.unwrap().unwrap();
        }
        let events = s.read().await.unwrap();
        assert_eq!(events.len(), 50);
        let mut seen = std::collections::HashSet::new();
        for ev in &events {
            let i = ev["i"].as_i64().unwrap();
            assert!(seen.insert(i), "duplicate seq {i}");
        }
        assert_eq!(seen.len(), 50);
    }

    /// `read_from(N)` is byte-equivalent to the legacy `read()`
    /// when `N = 0` and to the suffix strictly past event `N`
    /// otherwise. The cursor the server resumes from is the
    /// last `last_event_seq` it observed.
    #[tokio::test]
    async fn in_memory_sink_read_from_zero_matches_read() {
        let s = InMemorySessionSink::new("test");
        for i in 0..3 {
            s.append(&json!({"i": i})).await.unwrap();
        }
        let all = s.read().await.unwrap();
        let from_zero = s.read_from(0).await.unwrap();
        assert_eq!(all, from_zero, "`read_from(0)` must mirror `read()`");
    }

    #[tokio::test]
    async fn in_memory_sink_read_from_n_returns_strict_suffix() {
        let s = InMemorySessionSink::new("test");
        for i in 0..5 {
            s.append(&json!({"i": i})).await.unwrap();
        }
        // Event sequence is 1-indexed (first `append` ⇒ seq=1).
        // `read_from(N)` drops events with `seq ≤ N`.
        let from_two = s.read_from(2).await.unwrap();
        assert_eq!(from_two.len(), 3);
        assert_eq!(from_two[0]["i"], 2);
        assert_eq!(from_two[1]["i"], 3);
        assert_eq!(from_two[2]["i"], 4);
        let from_four = s.read_from(4).await.unwrap();
        assert_eq!(from_four.len(), 1);
        assert_eq!(from_four[0]["i"], 4);
    }

    #[tokio::test]
    async fn in_memory_sink_read_from_past_end_returns_empty() {
        let s = InMemorySessionSink::new("test");
        for i in 0..2 {
            s.append(&json!({"i": i})).await.unwrap();
        }
        // `seq > last_event_seq` ⇒ empty slice. The HTTP seam
        // surfaces that mismatch as `410 Gone` before reaching
        // here, but the sink itself stays defensive: a stale
        // cursor must not panic.
        let beyond = s.read_from(99).await.unwrap();
        assert!(beyond.is_empty());
    }

    // -----------------------------------------------------------------
    // Idempotency (`append_with_key`)
    // -----------------------------------------------------------------

    #[tokio::test]
    async fn in_memory_sink_idempotent_append_returns_original_seq() {
        let s = InMemorySessionSink::new("idem");
        let first = s
            .append_with_key(Some("evt-1"), &json!({"i": 0}))
            .await
            .unwrap();
        let second = s
            .append_with_key(Some("evt-1"), &json!({"i": 0}))
            .await
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(s.snapshot().await.unwrap().last_event_seq, 1);
        assert_eq!(s.read().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn in_memory_sink_idempotent_distinct_keys_both_written() {
        let s = InMemorySessionSink::new("idem-distinct");
        s.append_with_key(Some("a"), &json!({"i": 0}))
            .await
            .unwrap();
        s.append_with_key(Some("b"), &json!({"i": 1}))
            .await
            .unwrap();
        s.append_with_key(Some("a"), &json!({"i": 0}))
            .await
            .unwrap();
        assert_eq!(s.snapshot().await.unwrap().last_event_seq, 2);
    }

    #[tokio::test]
    async fn in_memory_sink_idempotent_none_key_legacy_behavior() {
        let s = InMemorySessionSink::new("idem-none");
        s.append_with_key(None, &json!({"i": 0})).await.unwrap();
        s.append_with_key(None, &json!({"i": 0})).await.unwrap();
        assert_eq!(s.snapshot().await.unwrap().last_event_seq, 2);
    }

    // -----------------------------------------------------------------
    // Batched append (`append_many`)
    // -----------------------------------------------------------------

    #[tokio::test]
    async fn in_memory_sink_append_many_writes_batch() {
        let s = InMemorySessionSink::new("many");
        let batch: Vec<Value> = (0..10).map(|i| json!({"i": i})).collect();
        let last = s.append_many(&batch).await.unwrap();
        assert_eq!(last, 10);
        assert_eq!(s.snapshot().await.unwrap().last_event_seq, 10);
        assert_eq!(s.read().await.unwrap().len(), 10);
    }

    #[tokio::test]
    async fn in_memory_sink_append_many_empty_is_noop() {
        let s = InMemorySessionSink::new("many-empty");
        let last = s.append_many(&[]).await.unwrap();
        assert_eq!(last, 0);
        assert!(s.is_empty());
    }

    #[tokio::test]
    async fn in_memory_sink_append_many_rejects_after_close() {
        let s = InMemorySessionSink::new("many-closed");
        s.close(SessionEndReason::Completed).await.unwrap();
        let err = s.append_many(&[json!({"x": 1})]).await.unwrap_err();
        assert_eq!(err, SessionError::Closed);
    }
}
