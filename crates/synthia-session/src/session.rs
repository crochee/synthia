//! Session management module
//!
//! This module provides session management functionality for conversation
//! session lifecycle management. The legacy `pub trait SessionManager`
//! (12 methods) was REMOVED 2026-06-15 in change
//! `2026-06-15-p0-trait-review-remediation` Sub-task C because it had
//! 0 trait bound usage, 0 dyn dispatch, and 1 real impl — pure YAGNI.
//!
//! Concrete session persistence is provided by `SessionFileStore`
//! (in `crate::file_store`) and the `SessionManager` struct
//! (in `crate::manager`).

use backoff::ExponentialBackoff;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use synthia_provider::Message;

use crate::SessionConfig;

/// Type alias for session operation results.
pub type Result<T> = std::result::Result<T, SessionError>;

/// Error type for session operations.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Session error: {0}")]
    Session(String),

    #[error("Session not found")]
    NotFound,

    #[error("Unauthorized")]
    Unauthorized,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl SessionError {
    pub fn session(msg: impl Into<String>) -> Self {
        Self::Session(msg.into())
    }
}

pub fn default_max_steps() -> u32 {
    50
}

pub fn default_backoff() -> ExponentialBackoff {
    ExponentialBackoff {
        initial_interval: std::time::Duration::from_millis(200),
        max_interval: std::time::Duration::from_secs(2),
        max_elapsed_time: Some(std::time::Duration::from_secs(10)),
        ..ExponentialBackoff::default()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: Option<String>,
    pub parent_id: Option<String>,
    #[serde(default = "default_max_steps")]
    pub max_steps: u32,
    #[serde(skip, default = "default_backoff")]
    pub backoff: ExponentialBackoff,
    pub max_context_tokens: Option<usize>,
    pub compaction_threshold: Option<f64>,
    pub conversation: Vec<Message>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Clone for Session {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            name: self.name.clone(),
            parent_id: self.parent_id.clone(),
            max_steps: self.max_steps,
            backoff: default_backoff(),
            max_context_tokens: self.max_context_tokens,
            compaction_threshold: self.compaction_threshold,
            conversation: self.conversation.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        let now = Utc::now().timestamp();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: None,
            parent_id: None,
            max_steps: default_max_steps(),
            backoff: default_backoff(),
            max_context_tokens: None,
            compaction_threshold: None,
            conversation: vec![],
            created_at: now,
            updated_at: now,
        }
    }
}

impl From<Session> for SessionConfig {
    fn from(session: Session) -> Self {
        Self {
            model: String::new(),
            max_tokens: session.max_context_tokens.unwrap_or(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Session Tests
    // =========================================================================

    #[test]
    fn test_session_default() {
        let session = Session::default();
        assert!(!session.id.is_empty());
        assert!(session.name.is_none());
        assert!(session.parent_id.is_none());
        assert_eq!(session.max_steps, 50);
        assert!(session.max_context_tokens.is_none());
        assert!(session.compaction_threshold.is_none());
        assert!(session.conversation.is_empty());
        assert_eq!(session.created_at, session.updated_at);
    }

    #[test]
    fn test_session_id_is_uuid() {
        let session = Session::default();
        // UUID format: 8-4-4-4-12 hex digits
        let uuid_parts: Vec<&str> = session.id.split('-').collect();
        assert_eq!(uuid_parts.len(), 5);
        assert_eq!(uuid_parts[0].len(), 8);
        assert_eq!(uuid_parts[1].len(), 4);
        assert_eq!(uuid_parts[2].len(), 4);
        assert_eq!(uuid_parts[3].len(), 4);
        assert_eq!(uuid_parts[4].len(), 12);
    }

    #[test]
    fn test_session_updated_at_differs() {
        let session = Session::default();
        // created_at and updated_at should be equal initially
        assert_eq!(session.created_at, session.updated_at);
    }

    #[test]
    fn test_session_from_session_config() {
        let session = Session {
            id: "test-id".to_string(),
            name: Some("Test Session".to_string()),
            parent_id: Some("parent-id".to_string()),
            max_steps: 100,
            backoff: default_backoff(),
            max_context_tokens: Some(4096),
            compaction_threshold: Some(0.8),
            conversation: vec![],
            created_at: 1000,
            updated_at: 1000,
        };

        let config: SessionConfig = session.into();
        assert_eq!(config.model, "");
        assert_eq!(config.max_tokens, 4096);
    }

    #[test]
    fn test_session_serialization() {
        let session = Session::default();
        let json = serde_json::to_string(&session).unwrap();
        let deserialized: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, session.id);
        assert_eq!(deserialized.name, session.name);
        assert_eq!(deserialized.max_steps, session.max_steps);
    }

    #[test]
    fn test_session_with_conversation_serialization() {
        let mut session = Session::default();
        session
            .conversation
            .push(synthia_provider::Message::user("Hello"));
        session
            .conversation
            .push(synthia_provider::Message::assistant("Hi there!"));

        let json = serde_json::to_string(&session).unwrap();
        let deserialized: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.conversation.len(), 2);
    }

    #[test]
    fn test_session_with_all_fields() {
        let mut session = Session {
            name: Some("My Session".to_string()),
            parent_id: Some("parent-123".to_string()),
            max_steps: 200,
            max_context_tokens: Some(8192),
            compaction_threshold: Some(0.75),
            ..Default::default()
        };
        session
            .conversation
            .push(synthia_provider::Message::user("Test"));

        assert_eq!(session.name, Some("My Session".to_string()));
        assert_eq!(session.parent_id, Some("parent-123".to_string()));
        assert_eq!(session.max_steps, 200);
        assert_eq!(session.max_context_tokens, Some(8192));
        assert_eq!(session.compaction_threshold, Some(0.75));
        assert_eq!(session.conversation.len(), 1);
    }

    // =========================================================================
    // Helper Function Tests
    // =========================================================================

    #[test]
    fn test_default_max_steps() {
        let max_steps = default_max_steps();
        assert_eq!(max_steps, 50);
    }

    #[test]
    fn test_default_backoff() {
        let backoff = default_backoff();
        assert_eq!(
            backoff.initial_interval,
            std::time::Duration::from_millis(200)
        );
        assert_eq!(backoff.max_interval, std::time::Duration::from_secs(2));
        assert_eq!(
            backoff.max_elapsed_time,
            Some(std::time::Duration::from_secs(10))
        );
    }

    #[test]
    fn test_default_backoff_exponential_growth() {
        let backoff = default_backoff();
        // Verify the backoff structure has expected ratio for exponential growth
        let ratio = backoff.max_interval.as_millis() as f64
            / backoff.initial_interval.as_millis() as f64;
        assert_eq!(ratio, 10.0); // 2000ms / 200ms = 10
    }
}
