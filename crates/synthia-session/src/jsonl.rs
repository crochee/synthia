//! `JsonlSessionSink` — the on-disk production backend.
//!
//! Persists events to `{dir}/events.jsonl`. Every `append` is
//! followed by an `fsync`, so `append().await` returning `Ok`
//! guarantees the bytes are durable on the local filesystem.
//!
//! ## Layout
//!
//! ```text
//! {root_dir}/
//!   events.jsonl    # one JSON value per line, chronologically
//!                   # ordered; first line is the session metadata
//!                   # header (see SessionMetadataHeader below).
//! ```
//!
//! ## Reading
//!
//! `read()` scans the file in chronological order and returns
//! every line except the metadata header. The header is read
//! lazily by `snapshot()`.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::sink::{
    SessionEndReason,
    SessionError,
    SessionSink,
    SessionSnapshot,
};

/// On-disk JSONL-backed sink. Cheap to clone — wraps shared
/// `Arc<Mutex<State>>`.
///
/// The mutex is `tokio::sync::Mutex` (not `std::sync::Mutex`)
/// so the lock can be held across the `spawn_blocking` IO
/// branches without requiring `Send` of the guard. Holding
/// the mutex across `append` and `read` is intentional: it
/// serializes the file, so a concurrent `read` can never
/// observe a half-written line while another thread is
/// appending.
#[derive(Clone)]
pub struct JsonlSessionSink {
    root: PathBuf,
    id: String,
    state: Arc<Mutex<State>>,
}

struct State {
    closed: bool,
    next_seq: u64,
    bytes_on_disk: u64,
}

impl JsonlSessionSink {
    /// Create (or open) a JSONL sink rooted at `dir`. The directory
    /// is created if missing. Existing `events.jsonl` files are
    /// appended to (so resumes preserve history).
    pub fn new(id: impl Into<String>, dir: impl Into<PathBuf>) -> Self {
        let root: PathBuf = dir.into();
        let _ = fs::create_dir_all(&root);
        let bytes_on_disk = root
            .join("events.jsonl")
            .metadata()
            .map(|m| m.len())
            .unwrap_or(0);
        let next_seq = count_lines(&root.join("events.jsonl"));
        Self {
            root,
            id: id.into(),
            state: Arc::new(Mutex::new(State {
                closed: false,
                next_seq,
                bytes_on_disk,
            })),
        }
    }

    fn events_path(&self) -> PathBuf {
        self.root.join("events.jsonl")
    }
}

#[async_trait]
impl SessionSink for JsonlSessionSink {
    fn id(&self) -> &str {
        &self.id
    }

    async fn append(&self, event: &Value) -> Result<(), SessionError> {
        let mut state = self.state.lock().await;
        if state.closed {
            return Err(SessionError::Closed);
        }
        let line = serde_json::to_string(event).map_err(|e| {
            SessionError::AppendFailed(format!("serialize event: {e}"))
        })?;
        let path = self.events_path();
        state.next_seq += 1;
        let bytes = line.len() as u64 + 1;

        // Phase 2: write + fsync on the blocking thread pool.
        // The mutex is held across the await so a concurrent
        // `read` from another task waits until the fsync
        // completes; this is what guarantees `read` never
        // sees a half-written line.
        let result =
            tokio::task::spawn_blocking(move || append_line_sync(&path, &line))
                .await
                .map_err(|e| {
                    SessionError::AppendFailed(format!(
                        "join blocking task: {e}"
                    ))
                })?;

        if let Err(e) = result {
            // Roll back the reserved seq so snapshots stay
            // consistent with the on-disk state.
            state.next_seq -= 1;
            return Err(SessionError::AppendFailed(e));
        }
        state.bytes_on_disk += bytes;
        Ok(())
    }

    async fn read(&self) -> Result<Vec<Value>, SessionError> {
        // Acquire the same mutex that `append` holds during
        // its fsync. This serializes read against write at
        // the file level, so we never observe a half-written
        // line. The actual `fs::read` is moved off-thread so
        // we do not block the runtime worker.
        let _guard = self.state.lock().await;
        let path = self.events_path();
        let bytes = tokio::task::spawn_blocking(move || {
            fs::read(&path).map_err(|e| {
                SessionError::ReadFailed(format!("read events.jsonl: {e}"))
            })
        })
        .await
        .map_err(|e| {
            SessionError::ReadFailed(format!("join blocking task: {e}"))
        })??;
        let mut out = Vec::new();
        for line in bytes.split(|b| *b == b'\n') {
            if line.is_empty() {
                continue;
            }
            let v = serde_json::from_slice::<Value>(line).map_err(|e| {
                SessionError::ReadFailed(format!("parse jsonl line: {e}"))
            })?;
            out.push(v);
        }
        Ok(out)
    }

    async fn snapshot(&self) -> Result<SessionSnapshot, SessionError> {
        let state = self.state.lock().await;
        Ok(SessionSnapshot {
            session_id: self.id.clone(),
            last_event_seq: state.next_seq,
            bytes_on_disk: state.bytes_on_disk,
        })
    }

    async fn close(
        &self,
        _reason: SessionEndReason,
    ) -> Result<(), SessionError> {
        let mut state = self.state.lock().await;
        state.closed = true;
        Ok(())
    }
}

/// Synchronous append + fsync. Blocking is intentional — the
/// `write-through` contract requires that `Ok` means "durable on
/// disk". Putting this on a blocking IO thread is the caller's
/// responsibility; `synthia-server` wraps `append` in
/// `tokio::task::spawn_blocking`.
fn append_line_sync(path: &Path, line: &str) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("open {}: {e}", path.display()))?;
    writeln!(file, "{line}").map_err(|e| format!("write: {e}"))?;
    file.sync_all().map_err(|e| format!("fsync: {e}"))?;
    Ok(())
}

fn count_lines(path: &Path) -> u64 {
    let Ok(bytes) = fs::read(path) else {
        return 0;
    };
    if bytes.is_empty() {
        return 0;
    }
    bytes.iter().filter(|b| **b == b'\n').count() as u64
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tempfile::Builder;

    use super::*;

    fn temp_dir() -> PathBuf {
        let tmp = Builder::new().disable_cleanup(true).tempdir().unwrap();
        let p = tmp.path().to_path_buf();
        drop(tmp);
        p
    }

    #[tokio::test]
    async fn jsonl_sink_round_trips_events_across_instances() {
        let dir = temp_dir().join("s1");
        let sink = JsonlSessionSink::new("s1", dir.clone());
        sink.append(&json!({"role": "user", "text": "hi"}))
            .await
            .unwrap();
        sink.append(&json!({"role": "assistant", "text": "hello"}))
            .await
            .unwrap();
        // Re-open the same dir — should see the same two events.
        let sink2 = JsonlSessionSink::new("s1", dir);
        let events = sink2.read().await.unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["text"], "hi");
        assert_eq!(events[1]["text"], "hello");
    }

    #[tokio::test]
    async fn jsonl_sink_rejects_appends_after_close() {
        let dir = temp_dir().join("s2");
        let sink = JsonlSessionSink::new("s2", dir);
        sink.close(SessionEndReason::Completed).await.unwrap();
        let err = sink.append(&json!({"x": 1})).await.unwrap_err();
        assert_eq!(err, SessionError::Closed);
    }

    #[tokio::test]
    async fn jsonl_sink_persists_across_appends_in_sequence_order() {
        let dir = temp_dir().join("s3");
        let sink = JsonlSessionSink::new("s3", dir);
        for i in 0..5 {
            sink.append(&json!({"i": i})).await.unwrap();
        }
        let snap = sink.snapshot().await.unwrap();
        assert_eq!(snap.last_event_seq, 5);
        let events = sink.read().await.unwrap();
        for (idx, ev) in events.iter().enumerate() {
            assert_eq!(ev["i"], idx as i64);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn jsonl_sink_concurrent_appends_preserve_all_records() {
        // 50 concurrent appends across 4 worker threads. Each
        // append runs through `spawn_blocking` (IO + fsync) and
        // the prior race between `read()` and `append()`
        // (half-written last line) would surface here.
        let dir = temp_dir().join("s_concurrent");
        let sink = JsonlSessionSink::new("s_concurrent", dir);
        let mut handles = Vec::new();
        for i in 0..50 {
            let s = sink.clone();
            handles.push(tokio::spawn(async move {
                s.append(&json!({"i": i})).await
            }));
        }
        for h in handles {
            h.await.unwrap().unwrap();
        }
        let snap = sink.snapshot().await.unwrap();
        assert_eq!(snap.last_event_seq, 50);
        let events = sink.read().await.unwrap();
        assert_eq!(events.len(), 50);
        // Every seq must appear exactly once.
        let mut seen = std::collections::HashSet::new();
        for ev in &events {
            let i = ev["i"].as_i64().unwrap();
            assert!(seen.insert(i), "duplicate seq {i}");
        }
        assert_eq!(seen.len(), 50);
    }
}
