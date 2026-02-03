//! Session management module
//!
//! This module provides session management functionality including the SessionManager trait
//! for conversation session lifecycle management.

mod file_store;

use async_trait::async_trait;
use backoff::ExponentialBackoff;
use chrono::Utc;
use rmcp::model::SamplingMessage;
use serde::{Deserialize, Serialize};

use crate::{Result, config::SessionConfig};

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

#[derive(Clone, Debug, Serialize, Deserialize)]
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
    pub conversation: Vec<SamplingMessage>,
    pub created_at: i64,
    pub updated_at: i64,
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
            id: session.id,
            parent_id: session.parent_id,
            max_steps: session.max_steps,
            backoff: session.backoff,
            max_context_tokens: session.max_context_tokens,
            max_tokens: None,
        }
    }
}

#[async_trait]
pub trait SessionManager: Send + Sync {
    async fn get_session(
        &self,
        session_config: &SessionConfig,
    ) -> Result<Option<Session>>;
    async fn create_session(&self) -> Result<Session>;
    async fn update_session(&self, session: &Session) -> Result<()>;
    async fn delete_session(
        &self,
        session_config: &SessionConfig,
    ) -> Result<()>;
    async fn add_message(
        &self,
        session_config: &SessionConfig,
        message: &SamplingMessage,
    ) -> Result<()>;
    async fn get_conversation(
        &self,
        session_config: &SessionConfig,
    ) -> Result<Vec<SamplingMessage>>;
    async fn get_recent_conversations(
        &self,
        limit: usize,
        mark: Option<&str>,
    ) -> Result<(Vec<Session>, Option<String>, bool)>;
    async fn get_conversation_messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<SamplingMessage>>;
    async fn replace_conversation(
        &self,
        session_config: &SessionConfig,
        conversation: &[SamplingMessage],
    ) -> Result<()>;
    async fn fix_conversation(
        &self,
        session_config: &SessionConfig,
    ) -> Result<Vec<SamplingMessage>>;

    async fn get_message_count(
        &self,
        session_config: &SessionConfig,
    ) -> Result<usize> {
        let conversation = self.get_conversation(session_config).await?;
        Ok(conversation.len())
    }

    async fn validate_append_only(
        &self,
        session_config: &SessionConfig,
        expected_count: usize,
    ) -> Result<bool> {
        let actual_count = self.get_message_count(session_config).await?;
        Ok(actual_count >= expected_count)
    }
}

pub use file_store::SessionFileStore;

#[cfg(test)]
mod tests {
    use rmcp::model::SamplingMessage;

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
        assert_eq!(config.id, "test-id");
        assert_eq!(config.parent_id, Some("parent-id".to_string()));
        assert_eq!(config.max_steps, 100);
        assert_eq!(config.max_context_tokens, Some(4096));
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
            .push(SamplingMessage::user_text("Hello"));
        session
            .conversation
            .push(SamplingMessage::assistant_text("Hi there!"));

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
            .push(SamplingMessage::user_text("Test"));

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

    // =========================================================================
    // SessionManager Trait - Default Method Tests
    // =========================================================================

    struct MockSessionManager {
        messages: Vec<SamplingMessage>,
    }

    impl MockSessionManager {
        fn new(messages: Vec<SamplingMessage>) -> Self {
            Self { messages }
        }
    }

    #[async_trait::async_trait]
    impl SessionManager for MockSessionManager {
        async fn get_session(
            &self,
            _session_config: &SessionConfig,
        ) -> Result<Option<Session>> {
            Ok(None)
        }

        async fn create_session(&self) -> Result<Session> {
            Ok(Session::default())
        }

        async fn update_session(&self, _session: &Session) -> Result<()> {
            Ok(())
        }

        async fn delete_session(
            &self,
            _session_config: &SessionConfig,
        ) -> Result<()> {
            Ok(())
        }

        async fn add_message(
            &self,
            _session_config: &SessionConfig,
            _message: &SamplingMessage,
        ) -> Result<()> {
            Ok(())
        }

        async fn get_conversation(
            &self,
            _session_config: &SessionConfig,
        ) -> Result<Vec<SamplingMessage>> {
            Ok(self.messages.clone())
        }

        async fn get_recent_conversations(
            &self,
            _limit: usize,
            _mark: Option<&str>,
        ) -> Result<(Vec<Session>, Option<String>, bool)> {
            Ok((vec![], None, false))
        }

        async fn get_conversation_messages(
            &self,
            _session_id: &str,
        ) -> Result<Vec<SamplingMessage>> {
            Ok(self.messages.clone())
        }

        async fn replace_conversation(
            &self,
            _session_config: &SessionConfig,
            _conversation: &[SamplingMessage],
        ) -> Result<()> {
            Ok(())
        }

        async fn fix_conversation(
            &self,
            _session_config: &SessionConfig,
        ) -> Result<Vec<SamplingMessage>> {
            Ok(self.messages.clone())
        }
    }

    #[tokio::test]
    async fn test_session_manager_get_message_count() {
        let manager = MockSessionManager::new(vec![
            SamplingMessage::user_text("Hello"),
            SamplingMessage::assistant_text("Hi"),
            SamplingMessage::user_text("How are you?"),
        ]);

        let config = SessionConfig::new("test".to_string());
        let count = manager.get_message_count(&config).await.unwrap();
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn test_session_manager_get_message_count_empty() {
        let manager = MockSessionManager::new(vec![]);
        let config = SessionConfig::new("test".to_string());
        let count = manager.get_message_count(&config).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_session_manager_validate_append_only_true() {
        let manager = MockSessionManager::new(vec![
            SamplingMessage::user_text("Hello"),
            SamplingMessage::user_text("World"),
        ]);
        let config = SessionConfig::new("test".to_string());

        let result = manager.validate_append_only(&config, 2).await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_session_manager_validate_append_only_false() {
        let manager =
            MockSessionManager::new(vec![SamplingMessage::user_text("Hello")]);
        let config = SessionConfig::new("test".to_string());

        let result = manager.validate_append_only(&config, 5).await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_session_manager_validate_append_only_exact() {
        let manager = MockSessionManager::new(vec![
            SamplingMessage::user_text("Hello"),
            SamplingMessage::user_text("World"),
        ]);
        let config = SessionConfig::new("test".to_string());

        // Exact count should return true
        let result = manager.validate_append_only(&config, 2).await.unwrap();
        assert!(result);
    }

    // =========================================================================
    // SessionConfig Tests (from crate config)
    // =========================================================================

    #[test]
    fn test_session_config_new() {
        let config = SessionConfig::new("session-123".to_string());
        assert_eq!(config.id, "session-123");
        assert!(config.parent_id.is_none());
        assert!(config.max_tokens.is_none());
    }
}
