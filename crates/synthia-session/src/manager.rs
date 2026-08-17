//! In-memory registry that owns `SessionSink` instances.
//!
//! After the panel/session refactor each session is identified
//! by `(user_id, session_id)` and persisted through a
//! [`SessionSink`] rooted at
//! `<sessions_root>/<user_id>/<session_id>/`. The registry:
//!
//! 1. Eagerly creates the sink when a session is created, so
//!    the on-disk directory exists before the controller
//!    starts appending events.
//! 2. Hands out the same `Arc<dyn SessionSink>` for repeated
//!    lookups of the same session (cheap clones).
//! 3. Provides a shared [`InputQueue`] for the controller's
//!    "prompt / steer" command channel.
//!
//! `SessionRegistry` is intentionally minimal: it does NOT
//! store state machines, budgets, approvals, or any policy
//! concerns. Those live in `synthia-server::session::SessionController`.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use parking_lot::RwLock;
use serde_json::Value;

use crate::{SessionError, SessionSink, jsonl::JsonlSessionSink};

// ---------------------------------------------------------------------------
// Session metadata
// ---------------------------------------------------------------------------

/// Minimal session metadata kept in memory. After the refactor
/// the only fields server callers still consult are `id` and
/// `user_id`; everything else lives in the sink.
#[derive(Clone, Debug)]
pub struct Session {
    pub id: String,
    pub user_id: String,
}

// ---------------------------------------------------------------------------
// SessionRegistry
// ---------------------------------------------------------------------------

/// Owns the per-session sink registry, the in-memory session
/// index, and the shared input queue.
pub struct SessionRegistry {
    sessions_root: PathBuf,
    sessions: RwLock<HashMap<String, Session>>,
    sinks: RwLock<HashMap<String, Arc<dyn SessionSink>>>,
    input_queue: InputQueue,
}

impl SessionRegistry {
    /// Build a registry rooted at `sessions_root`.
    pub fn new(sessions_root: PathBuf) -> Self {
        Self {
            sessions_root,
            sessions: RwLock::new(HashMap::new()),
            sinks: RwLock::new(HashMap::new()),
            input_queue: InputQueue::default(),
        }
    }

    /// Path of the sessions root.
    pub fn sessions_root(&self) -> &std::path::Path {
        &self.sessions_root
    }

    /// Get-or-create the on-disk sink for
    /// `(user_id, session_id)`. The sink is the **only**
    /// durable source of truth after the refactor.
    ///
    /// Uses a single `write()` lock with `entry().or_insert_with(...)`
    /// so concurrent first-lookups for the same session always
    /// converge on the same sink. The previous version used
    /// double-checked locking on a `RwLock` which had a race
    /// where two threads could each create (and register)
    /// different sinks for the same key.
    pub fn sink(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Arc<dyn SessionSink> {
        let key = sink_key(user_id, session_id);
        self.sinks
            .write()
            .entry(key)
            .or_insert_with(|| {
                let dir = self.sessions_root.join(user_id).join(session_id);
                Arc::new(JsonlSessionSink::new(session_id, dir))
            })
            .clone()
    }

    /// Register a new session. Inserts the metadata into the
    /// in-memory index and eagerly creates the sink so the
    /// on-disk directory exists before the controller starts
    /// appending events.
    pub async fn create_with_user(
        &self,
        id: String,
        user_id: String,
    ) -> Result<Session, SessionError> {
        if user_id.is_empty() {
            return Err(SessionError::Invalid("empty user_id".into()));
        }
        let session = Session {
            id: id.clone(),
            user_id: user_id.clone(),
        };
        self.sessions.write().insert(id.clone(), session.clone());
        // Touch the sink so the dir is created eagerly.
        let _ = self.sink(&user_id, &id);
        Ok(session)
    }

    /// Look up a session by id.
    pub async fn get(&self, id: &str) -> Option<Session> {
        self.sessions.read().get(id).cloned()
    }

    /// Shared input queue used by the controller to bridge
    /// `submit(Prompt | Steer)` calls to the run loop.
    pub fn input_queue(&self) -> InputQueue {
        self.input_queue.clone()
    }
}

/// Compose the internal key for the sinks registry.
fn sink_key(user_id: &str, session_id: &str) -> String {
    format!("{user_id}::{session_id}")
}

/// Legacy constant: the user_id used for single-tenant
/// deployments. Preserved so `synthia-server` auth middleware
/// keeps compiling.
pub const SERVER_DEFAULT_USER_ID: &str = "dev";

// ---------------------------------------------------------------------------
// InputQueue
// ---------------------------------------------------------------------------

/// In-memory per-session input queue. The steering channel is
/// owned by the agent loop (not the sink), but the
/// `SessionController` still needs a small queue to bridge
/// between `submit(Prompt | Steer)` calls and the run loop.
///
/// Internally backed by a `tokio::sync::Mutex` so that the
/// registry is safe to share across `.await` points without
/// blocking the runtime worker thread.
#[derive(Clone, Default)]
pub struct InputQueue {
    inner: Arc<tokio::sync::Mutex<HashMap<String, Vec<PendingEntry>>>>,
}

impl InputQueue {
    /// Push a new pending entry onto the session's queue.
    /// Returns the entries that were *also* queued at the
    /// same time (an artifact of the previous
    /// `VecDeque`-based API; preserved so the call site
    /// keeps compiling).
    ///
    /// Async-friendly: awaits the lock without blocking the
    /// runtime worker.
    pub async fn push(
        &self,
        _user_id: &str,
        session_id: &str,
        value: Value,
        _priority: Option<()>,
    ) -> Result<Vec<PendingEntry>, SessionError> {
        let entry = PendingEntry::from_value(value);
        let mut guard = self.inner.lock().await;
        let bucket = guard.entry(session_id.to_string()).or_default();
        bucket.push(entry.clone());
        Ok(vec![entry])
    }

    /// Whether the session has any pending input. The
    /// controller uses this to decide whether to start a
    /// run.
    pub async fn has_pending(&self, _user_id: &str, session_id: &str) -> bool {
        self.inner
            .lock()
            .await
            .get(session_id)
            .is_some_and(|v| !v.is_empty())
    }

    /// Drain all pending entries for the session. After the
    /// call the queue is empty.
    pub async fn drain_pending(
        &self,
        _user_id: &str,
        session_id: &str,
    ) -> Result<Vec<PendingEntry>, SessionError> {
        let mut guard = self.inner.lock().await;
        Ok(guard.remove(session_id).unwrap_or_default())
    }
}

/// One entry in the input queue. The controller folds
/// `content` into the next user message.
#[derive(Clone, Debug)]
pub struct PendingEntry {
    pub content: String,
}

impl PendingEntry {
    fn from_value(value: Value) -> Self {
        let content = match value {
            Value::String(s) => s,
            other => other.to_string(),
        };
        Self { content }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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
    async fn registry_yields_same_sink_for_same_session_id() {
        let root = temp_dir();
        let r = SessionRegistry::new(root);
        let a = r.sink("alice", "s1");
        let b = r.sink("alice", "s1");
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[tokio::test]
    async fn registry_yields_distinct_sinks_for_distinct_sessions() {
        let root = temp_dir();
        let r = SessionRegistry::new(root);
        let a = r.sink("alice", "s1");
        let b = r.sink("alice", "s2");
        assert!(!Arc::ptr_eq(&a, &b));
    }

    #[tokio::test]
    async fn registry_sink_writes_to_disk() {
        let root = temp_dir();
        let r = SessionRegistry::new(root);
        let sink = r.sink("alice", "s1");
        sink.append(&json!({"role": "user", "text": "hi"}))
            .await
            .unwrap();
        let events = sink.read().await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["text"], "hi");
    }

    #[tokio::test]
    async fn create_with_user_populates_index_and_sink() {
        let root = temp_dir();
        let r = SessionRegistry::new(root);
        let s = r
            .create_with_user("s1".into(), "alice".into())
            .await
            .unwrap();
        assert_eq!(s.id, "s1");
        assert_eq!(s.user_id, "alice");
        let got = r.get("s1").await.unwrap();
        assert_eq!(got.user_id, "alice");
    }

    #[tokio::test]
    async fn create_with_user_rejects_empty_user_id() {
        let root = temp_dir();
        let r = SessionRegistry::new(root);
        let err = r
            .create_with_user("s1".into(), "".into())
            .await
            .unwrap_err();
        assert_eq!(err, SessionError::Invalid("empty user_id".into()));
    }

    #[tokio::test]
    async fn input_queue_round_trips() {
        let q = InputQueue::default();
        q.push("alice", "s1", Value::String("hello".into()), None)
            .await
            .unwrap();
        assert!(q.has_pending("alice", "s1").await);
        let drained = q.drain_pending("alice", "s1").await.unwrap();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].content, "hello");
        assert!(!q.has_pending("alice", "s1").await);
    }
}
