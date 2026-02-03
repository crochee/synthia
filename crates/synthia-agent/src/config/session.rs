//! Session configuration
//!
//! Configuration for session management and behavior.

use backoff::ExponentialBackoff;
use serde::{Deserialize, Serialize};

fn default_max_steps() -> u32 {
    50
}

fn default_backoff() -> ExponentialBackoff {
    ExponentialBackoff {
        initial_interval: std::time::Duration::from_millis(200),
        max_interval: std::time::Duration::from_secs(2),
        max_elapsed_time: Some(std::time::Duration::from_secs(10)),
        ..ExponentialBackoff::default()
    }
}

/// Configuration for session operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub id: String,

    pub parent_id: Option<String>,

    #[serde(default = "default_max_steps")]
    pub max_steps: u32,

    #[serde(skip, default = "default_backoff")]
    pub backoff: ExponentialBackoff,
    pub max_context_tokens: Option<usize>,
    /// Maximum tokens to use in total (for token budget enforcement)
    pub max_tokens: Option<u64>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            parent_id: None,
            max_steps: default_max_steps(),
            backoff: default_backoff(),
            max_context_tokens: None,
            max_tokens: None,
        }
    }
}

impl SessionConfig {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ..Default::default()
        }
    }

    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    pub fn with_max_steps(mut self, max_steps: u32) -> Self {
        self.max_steps = max_steps;
        self
    }

    pub fn with_max_context_tokens(mut self, tokens: usize) -> Self {
        self.max_context_tokens = Some(tokens);
        self
    }

    pub fn with_max_tokens(mut self, tokens: u64) -> Self {
        self.max_tokens = Some(tokens);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_config_default() {
        let config = SessionConfig::default();
        assert!(!config.id.is_empty());
        assert!(config.parent_id.is_none());
        assert_eq!(config.max_steps, 50);
        assert!(config.max_context_tokens.is_none());
        assert!(config.max_tokens.is_none());
    }

    #[test]
    fn test_session_config_builder() {
        let config = SessionConfig::new("test-session")
            .with_parent("parent-session")
            .with_max_steps(100);

        assert_eq!(config.id, "test-session");
        assert_eq!(config.parent_id, Some("parent-session".to_string()));
        assert_eq!(config.max_steps, 100);
    }

    #[test]
    fn test_session_config_builder_with_context_tokens() {
        let config =
            SessionConfig::new("test").with_max_context_tokens(100_000);

        assert_eq!(config.id, "test");
        assert_eq!(config.max_context_tokens, Some(100_000));
    }

    #[test]
    fn test_session_config_builder_with_max_tokens() {
        let config = SessionConfig::new("test").with_max_tokens(50_000);

        assert_eq!(config.max_tokens, Some(50_000));
    }

    #[test]
    fn test_session_config_builder_chaining() {
        let config = SessionConfig::new("chained")
            .with_parent("parent-id")
            .with_max_steps(200)
            .with_max_context_tokens(80_000)
            .with_max_tokens(40_000);

        assert_eq!(config.id, "chained");
        assert_eq!(config.parent_id, Some("parent-id".to_string()));
        assert_eq!(config.max_steps, 200);
        assert_eq!(config.max_context_tokens, Some(80_000));
        assert_eq!(config.max_tokens, Some(40_000));
    }

    #[test]
    fn test_session_config_new_with_id() {
        let config = SessionConfig::new("my-session-id");
        assert_eq!(config.id, "my-session-id");
        assert!(config.parent_id.is_none());
        assert_eq!(config.max_steps, 50);
    }

    #[test]
    fn test_session_config_uuid_generation() {
        // Default should generate a valid UUID
        let config = SessionConfig::default();
        let uuid_result = uuid::Uuid::parse_str(&config.id);
        assert!(
            uuid_result.is_ok(),
            "Default session ID should be a valid UUID"
        );
    }

    #[test]
    fn test_session_config_serialization() {
        let config = SessionConfig::new("serial-test")
            .with_max_steps(75)
            .with_max_context_tokens(50_000);

        let json = serde_json::to_string(&config).unwrap();
        let parsed: SessionConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, config.id);
        assert_eq!(parsed.max_steps, config.max_steps);
        // Note: backoff is skipped in serialization
    }

    #[test]
    fn test_session_config_deserialization() {
        let json = r#"{
            "id": "custom-id",
            "parent_id": "parent-123",
            "max_steps": 100,
            "max_context_tokens": 150000,
            "max_tokens": 75000
        }"#;
        let config: SessionConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config.id, "custom-id");
        assert_eq!(config.parent_id, Some("parent-123".to_string()));
        assert_eq!(config.max_steps, 100);
        assert_eq!(config.max_context_tokens, Some(150_000));
        assert_eq!(config.max_tokens, Some(75_000));
    }

    #[test]
    fn test_session_config_backoff_default() {
        let config = SessionConfig::default();
        // Verify backoff is set to expected defaults
        assert_eq!(
            config.backoff.initial_interval,
            std::time::Duration::from_millis(200)
        );
        assert_eq!(
            config.backoff.max_interval,
            std::time::Duration::from_secs(2)
        );
    }
}
