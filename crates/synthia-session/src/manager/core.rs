//! Core CRUD: create, get, remove, list, contains, len, is_empty.
//! User binding (create_with_user, assign_user, restore, delete)
//! lives here too because it is the only path that mutates the
//! `sessions` and `state_machines` maps together.

use std::{collections::HashMap, path::PathBuf, sync::RwLock, time::Duration};

use anyhow::{Result, anyhow};

use super::types::{CachedMessages, SessionSummary};
use crate::{
    error::StoreError,
    session::{Result as SessionResult, SessionError},
    state_machine::SessionStateMachine,
    store::{SessionInputQueue, Store},
    types::*,
};

/// `SessionManager` is the in-memory index of active sessions and
/// the per-session state machines. It is the only path through
/// which sessions are created, mutated, and torn down.
pub struct SessionManager {
    pub(super) sessions: RwLock<HashMap<String, Session>>,
    pub(super) state_machines: RwLock<HashMap<String, SessionStateMachine>>,
    pub(super) store: Store,
    pub(super) last_saved_offsets: RwLock<HashMap<String, usize>>,
    pub(super) approval_timers: RwLock<HashMap<String, tokio::time::Instant>>,
    pub(super) approval_timeout: Duration,
    pub(super) message_cache: RwLock<HashMap<String, CachedMessages>>,
    pub(super) cache_access_counter: RwLock<usize>,
    pub(super) input_queue: SessionInputQueue,
}

impl SessionManager {
    pub fn new(sessions_root: PathBuf) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            state_machines: RwLock::new(HashMap::new()),
            store: Store::new(sessions_root.clone()),
            last_saved_offsets: RwLock::new(HashMap::new()),
            approval_timers: RwLock::new(HashMap::new()),
            approval_timeout: Duration::from_secs(300),
            message_cache: RwLock::new(HashMap::new()),
            cache_access_counter: RwLock::new(0),
            input_queue: SessionInputQueue::new(sessions_root.clone()),
        }
    }

    /// Returns a clone of the [SessionInputQueue] for use by the
    /// agent's steering drain loop.
    pub fn input_queue(&self) -> crate::store::SessionInputQueue {
        self.input_queue.clone()
    }

    pub fn with_approval_timeout(mut self, timeout: Duration) -> Self {
        self.approval_timeout = timeout;
        self
    }

    pub async fn create(&self, id: String) -> Session {
        // `create` is a legacy entry point that does not know the
        // owning user. New callers MUST use `create_with_user` so the
        // session is bound to a concrete user_id namespace. When the
        // legacy `create` is used, the session is parked in the legacy
        // user namespace; the store refuses to persist it (see
        // `Store::save_metadata` -> `StoreError::EmptyUserId`) until a
        // caller promotes it via `assign_user`.
        let session = Session::new(id.clone());
        let sm = SessionStateMachine::new(
            id.clone(),
            self.store.clone(),
            SessionState::Initializing,
        );
        {
            let mut sessions = self.sessions.write().expect("RwLock poisoned");
            sessions.insert(id.clone(), session.clone());
        }
        {
            let mut state_machines =
                self.state_machines.write().expect("RwLock poisoned");
            state_machines.insert(id, sm);
        }
        session
    }

    /// Create a session bound to `user_id`. Prefer this over
    /// [`create`] for new code so the on-disk path is namespaced.
    pub async fn create_with_user(
        &self,
        id: String,
        user_id: String,
    ) -> Result<Session> {
        let session = Session::new_with_user(id.clone(), user_id.clone())?;
        let sm = SessionStateMachine::new(
            id.clone(),
            self.store.clone(),
            SessionState::Initializing,
        );
        {
            let mut sessions = self.sessions.write().expect("RwLock poisoned");
            sessions.insert(id.clone(), session.clone());
        }
        {
            let mut state_machines =
                self.state_machines.write().expect("RwLock poisoned");
            state_machines.insert(id, sm);
        }
        Ok(session)
    }

    /// Attach a `user_id` to a session previously created without one
    /// (e.g. via the legacy `create` constructor). Returns an error if
    /// the session already has a non-empty user_id or if the user_id
    /// is empty.
    pub async fn assign_user(
        &self,
        session_id: &str,
        user_id: String,
    ) -> Result<()> {
        if user_id.is_empty() {
            return Err(StoreError::EmptyUserId {
                session_id: session_id.to_string(),
            }
            .into());
        }
        let mut sessions = self.sessions.write().expect("RwLock poisoned");
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow!("session {session_id:?} not found"))?;
        if !session.user_id.is_empty() {
            return Err(anyhow!(
                "session {session_id:?} already bound to user {:?}",
                session.user_id
            ));
        }
        session.user_id = user_id;
        Ok(())
    }

    pub async fn restore(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Session> {
        if user_id.is_empty() {
            return Err(StoreError::EmptyUserId {
                session_id: session_id.to_string(),
            }
            .into());
        }
        let metadata = self.store.load_metadata(user_id, session_id)?;
        let _messages: Vec<serde_json::Value> =
            self.store.load_messages_recent(user_id, session_id, 1000)?;
        let session = Session {
            id: metadata.id.clone(),
            user_id: metadata.owner_user_id.clone(),
            state: metadata.state,
            token_usage: metadata.token_usage,
            created_at: metadata
                .created_at
                .parse()
                .unwrap_or_else(|_| chrono::Utc::now()),
            updated_at: metadata
                .updated_at
                .parse()
                .unwrap_or_else(|_| chrono::Utc::now()),
            config: metadata.config,
            needs_save: false,
            token_budget: TokenBudget::default(),
            context_window: 128_000,
            end_reason: metadata.end_reason.clone(),
            iteration: metadata.iteration,
            cumulative_tokens: metadata.cumulative_tokens,
            context_token_limit: metadata.context_token_limit,
            parent_id: metadata.parent_id.clone(),
        };
        let sm = SessionStateMachine::new(
            session_id.to_string(),
            self.store.clone(),
            session.state,
        );
        {
            let mut sessions = self.sessions.write().expect("RwLock poisoned");
            sessions.insert(session_id.to_string(), session.clone());
        }
        {
            let mut state_machines =
                self.state_machines.write().expect("RwLock poisoned");
            state_machines.insert(session_id.to_string(), sm);
        }
        Ok(session)
    }

    pub async fn get(&self, id: &str) -> Option<Session> {
        let sessions = self.sessions.read().expect("RwLock poisoned");
        sessions.get(id).cloned()
    }

    pub async fn remove(&self, id: &str) -> Option<Session> {
        {
            let mut state_machines =
                self.state_machines.write().expect("RwLock poisoned");
            state_machines.remove(id);
        }
        {
            let mut cache =
                self.message_cache.write().expect("RwLock poisoned");
            cache.remove(id);
        }
        let mut sessions = self.sessions.write().expect("RwLock poisoned");
        sessions.remove(id)
    }

    pub async fn list(&self) -> Vec<String> {
        let sessions = self.sessions.read().expect("RwLock poisoned");
        sessions.keys().cloned().collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.sessions
            .read()
            .expect("RwLock poisoned")
            .contains_key(name)
    }

    pub fn len(&self) -> usize {
        self.sessions.read().expect("RwLock poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.read().expect("RwLock poisoned").is_empty()
    }

    /// Delete a session from both disk and memory.
    /// Removes the session from the in-memory cache, state machine,
    /// message cache, and deletes the session directory from disk.
    pub async fn delete(&self, session_id: &str) -> Result<()> {
        // Look up the owning user BEFORE clearing in-memory state, otherwise
        // `user_id_for` would fail with "session not found" and we would
        // leak the on-disk directory.
        let user_id = self.user_id_for(session_id)?;

        {
            let mut sessions = self.sessions.write().expect("RwLock poisoned");
            sessions.remove(session_id);
        }
        {
            let mut state_machines =
                self.state_machines.write().expect("RwLock poisoned");
            state_machines.remove(session_id);
        }
        {
            let mut cache =
                self.message_cache.write().expect("RwLock poisoned");
            cache.remove(session_id);
        }
        {
            let mut offsets =
                self.last_saved_offsets.write().expect("RwLock poisoned");
            offsets.remove(session_id);
        }
        {
            let mut timers =
                self.approval_timers.write().expect("RwLock poisoned");
            timers.remove(session_id);
        }

        self.store.delete_session(user_id.as_str(), session_id)
    }

    /// Look up the `user_id` for an in-memory session. Returns
    /// `StoreError::EmptyUserId` if the session was created via the
    /// legacy `Session::new` path and has not yet been bound to a
    /// user (see [`assign_user`]).
    pub(super) fn user_id_for(&self, session_id: &str) -> Result<String> {
        let sessions = self.sessions.read().expect("RwLock poisoned");
        let session = sessions
            .get(session_id)
            .ok_or_else(|| anyhow!("session {session_id:?} not found"))?;
        if session.user_id.is_empty() {
            return Err(StoreError::EmptyUserId {
                session_id: session_id.to_string(),
            }
            .into());
        }
        Ok(session.user_id.clone())
    }

    /// List active sessions that belong to `user_id`.
    pub async fn list_for_user(&self, user_id: &str) -> Vec<SessionSummary> {
        let sessions = self.sessions.read().expect("RwLock poisoned");
        sessions
            .values()
            .filter(|s| s.user_id == user_id)
            .map(|s| SessionSummary {
                id: s.id.clone(),
                state: s.state,
                title: s.id.clone(),
                updated_at: s.updated_at,
                parent_id: s.parent_id.clone(),
            })
            .collect()
    }

    /// Create a new session as a child of `parent_session_id` under
    /// `user_id`. The parent must exist and belong to the caller.
    ///
    /// If `id` is `None`, a UUID is generated. The child session is
    /// persisted immediately so `list_children` can discover it.
    pub async fn create_child(
        &self,
        user_id: String,
        parent_session_id: String,
        id: Option<String>,
    ) -> Result<Session> {
        if user_id.is_empty() {
            return Err(StoreError::EmptyUserId {
                session_id: parent_session_id.clone(),
            }
            .into());
        }

        // Verify the parent exists and is owned by the caller.
        self.get_for_user(&user_id, &parent_session_id)
            .await
            .map_err(|_| {
                anyhow!(
                    "parent session {parent_session_id:?} not found for user {user_id:?}"
                )
            })?;

        let child_id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        self.create_with_user(child_id.clone(), user_id.clone())
            .await?;

        // Set the parent_id both in memory and on disk.
        {
            let mut sessions = self.sessions.write().expect("RwLock poisoned");
            let child = sessions.get_mut(&child_id).ok_or_else(|| {
                anyhow!("child session {child_id:?} disappeared")
            })?;
            child.parent_id = Some(parent_session_id);
        }

        let child = self
            .get_for_user(&user_id, &child_id)
            .await
            .map_err(|e| anyhow!("failed to reload child session: {e:?}"))?;
        self.save_metadata(&child)?;

        Ok(child)
    }

    /// List sessions that are children of `parent_session_id` and
    /// belong to `user_id`. Returns an error if `user_id` is empty or
    /// if the on-disk metadata indicates a cross-user access attempt.
    pub fn list_children(
        &self,
        user_id: &str,
        parent_session_id: &str,
    ) -> Result<Vec<SessionSummary>> {
        if user_id.is_empty() {
            return Err(StoreError::EmptyUserId {
                session_id: parent_session_id.to_string(),
            }
            .into());
        }

        let metas = self.store.list_sessions_with_metadata(user_id)?;
        Ok(metas
            .into_iter()
            .filter(|m| m.parent_id.as_deref() == Some(parent_session_id))
            .map(|m| {
                let id = m.id.clone();
                SessionSummary {
                    id,
                    state: m.state,
                    title: m.id,
                    updated_at: m
                        .updated_at
                        .parse()
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    parent_id: m.parent_id,
                }
            })
            .collect())
    }

    /// Get a session only if it belongs to `user_id`. Returns
    /// `SessionError::NotFound` for missing or non-owned sessions so
    /// existence is not leaked.
    pub async fn get_for_user(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> SessionResult<Session> {
        let sessions = self.sessions.read().expect("RwLock poisoned");
        let session = sessions.get(session_id).ok_or(SessionError::NotFound)?;
        if session.user_id != user_id {
            return Err(SessionError::NotFound);
        }
        Ok(session.clone())
    }

    /// Delete a session only if it belongs to `user_id`. Returns
    /// `SessionError::NotFound` for missing or non-owned sessions.
    pub async fn delete_for_user(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> SessionResult<()> {
        {
            let sessions = self.sessions.read().expect("RwLock poisoned");
            let session =
                sessions.get(session_id).ok_or(SessionError::NotFound)?;
            if session.user_id != user_id {
                return Err(SessionError::NotFound);
            }
        }

        self.remove(session_id).await;
        self.store
            .delete_session(user_id, session_id)
            .map_err(|e| SessionError::Session(e.to_string()))?;
        Ok(())
    }
}
