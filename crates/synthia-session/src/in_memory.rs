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

    async fn append(&self, event: &Value) -> Result<(), SessionError> {
        let mut s = self.state.lock().expect("InMemorySessionSink poisoned");
        if s.closed {
            return Err(SessionError::Closed);
        }
        s.seq += 1;
        s.events.push(event.clone());
        Ok(())
    }

    async fn read(&self) -> Result<Vec<Value>, SessionError> {
        let s = self.state.lock().expect("InMemorySessionSink poisoned");
        Ok(s.events.clone())
    }

    async fn snapshot(&self) -> Result<SessionSnapshot, SessionError> {
        let s = self.state.lock().expect("InMemorySessionSink poisoned");
        Ok(SessionSnapshot {
            session_id: self.id.clone(),
            last_event_seq: s.seq,
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
}
