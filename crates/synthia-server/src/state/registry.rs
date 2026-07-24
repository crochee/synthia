//! `AgentRegistry`: per-session agent state tracker.

use std::collections::HashMap;

use tokio::sync::RwLock;

use super::types::AgentSessionState;

/// Tracks per-session agent state.
///
/// Uses `RwLock<HashMap>` to allow concurrent reads while
/// serializing writes for state transitions.
#[derive(Default)]
pub struct AgentRegistry {
    sessions: RwLock<HashMap<String, AgentSessionState>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark a session as idle (created but not yet running).
    pub async fn register(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id.to_string(), AgentSessionState::Idle);
    }

    /// Transition a session to running state. Returns true if the
    /// session was previously idle.
    pub async fn start(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.write().await;
        if let Some(state) = sessions.get_mut(session_id)
            && *state == AgentSessionState::Idle
        {
            *state = AgentSessionState::Running;
            return true;
        }
        false
    }

    /// Mark a session as completed.
    pub async fn complete(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(state) = sessions.get_mut(session_id) {
            *state = AgentSessionState::Completed;
        }
    }

    /// Cancel a running session.
    pub async fn cancel(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(state) = sessions.get_mut(session_id) {
            *state = AgentSessionState::Cancelled;
        }
    }

    /// Get the current state of a session.
    pub async fn get(&self, session_id: &str) -> Option<AgentSessionState> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).copied()
    }

    /// Check if a session exists and is currently running.
    pub async fn is_running(&self, session_id: &str) -> bool {
        self.get(session_id).await == Some(AgentSessionState::Running)
    }

    /// Remove a session from the registry.
    pub async fn remove(&self, session_id: &str) -> Option<AgentSessionState> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id)
    }

    /// List all registered session IDs.
    pub async fn list_sessions(&self) -> Vec<String> {
        let sessions = self.sessions.read().await;
        sessions.keys().cloned().collect()
    }
}
