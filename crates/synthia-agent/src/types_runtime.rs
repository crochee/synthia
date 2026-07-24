// Re-export types defined in the config module.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use crate::{
    config::{AgentConfig, AgentConfigBuilder},
    events::{
        AgentEvent,
        AgentEventEmitter,
        AgentOutput,
        SessionEndReason,
        TokenUsage,
    },
    input::AgentInput,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reflection {
    pub iteration: usize,
    pub timestamp: DateTime<Utc>,
    pub summary: String,
    pub issues: Vec<String>,
    pub suggestions: Vec<String>,
}

impl Reflection {
    pub fn new(
        iteration: usize,
        summary: String,
        issues: Vec<String>,
        suggestions: Vec<String>,
    ) -> Self {
        Self {
            iteration,
            timestamp: Utc::now(),
            summary,
            issues,
            suggestions,
        }
    }
}

pub const REFLECTION_INTERVAL: usize = 5;
pub const REFLECTION_TOKEN_BUDGET_PERCENTAGE: f64 = 0.1;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use synthia_core::Error;
    use synthia_session::types::TokenBudget;

    use super::*;

    #[test]
    fn test_agent_config_default() {
        let config = AgentConfig::default();
        assert_eq!(config.model, "gpt-4o");
        assert_eq!(config.max_iterations, 90);
    }

    #[test]
    fn test_agent_config_validate_default() {
        let config = AgentConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_zero_max_iterations() {
        let config = AgentConfig {
            max_iterations: 0,
            ..AgentConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn test_validate_inverted_token_budget() {
        let budget = TokenBudget {
            hard_limit: 1000,
            soft_limit: 700,
            compaction_at: 600,
            must_compact_at: 900,
        };
        let config = AgentConfig {
            context_token_budget: Some(budget),
            ..AgentConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn test_validate_compaction_at_zero() {
        let budget = TokenBudget {
            hard_limit: 1000,
            soft_limit: 700,
            compaction_at: 0,
            must_compact_at: 900,
        };
        let config = AgentConfig {
            context_token_budget: Some(budget),
            ..AgentConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn test_validate_soft_limit_zero() {
        let budget = TokenBudget {
            hard_limit: 1000,
            soft_limit: 0,
            compaction_at: 850,
            must_compact_at: 900,
        };
        let config = AgentConfig {
            context_token_budget: Some(budget),
            ..AgentConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn test_validate_soft_limit_equals_compaction_at() {
        let budget = TokenBudget {
            hard_limit: 1000,
            soft_limit: 700,
            compaction_at: 700,
            must_compact_at: 900,
        };
        let config = AgentConfig {
            context_token_budget: Some(budget),
            ..AgentConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn test_builder_valid_config() {
        let config = AgentConfig::builder()
            .model("claude-3-opus".to_string())
            .max_iterations(50)
            .max_tokens(8192)
            .workspace_root(PathBuf::from("/tmp/test"))
            .build()
            .unwrap();

        assert_eq!(config.model, "claude-3-opus");
        assert_eq!(config.max_iterations, 50);
        assert_eq!(config.max_tokens, 8192);
        assert_eq!(config.workspace_root, PathBuf::from("/tmp/test"));
    }

    #[test]
    fn test_builder_invalid_max_iterations() {
        let err = AgentConfig::builder()
            .max_iterations(0)
            .build()
            .unwrap_err();

        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn test_builder_invalid_token_budget() {
        let bad_budget = TokenBudget {
            hard_limit: 1000,
            soft_limit: 700,
            compaction_at: 500,
            must_compact_at: 900,
        };
        let err = AgentConfig::builder()
            .token_budget_config(bad_budget)
            .build()
            .unwrap_err();

        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn test_builder_with_custom_token_budget() {
        let budget = TokenBudget {
            hard_limit: 200_000,
            soft_limit: 140_000,
            compaction_at: 170_000,
            must_compact_at: 180_000,
        };
        let config = AgentConfig::builder()
            .token_budget_config(budget)
            .build()
            .unwrap();

        let b = config.context_token_budget.as_ref().unwrap();
        assert_eq!(b.hard_limit, 200_000);
        assert_eq!(b.soft_limit, 140_000);
        assert_eq!(b.compaction_at, 170_000);
    }

    #[test]
    fn test_builder_all_fields() {
        let config = AgentConfig::builder()
            .model("gpt-4".to_string())
            .max_iterations(100)
            .max_tokens(4096)
            .temperature(0.7)
            .workspace_root(PathBuf::from("/workspace"))
            .token_budget(50_000)
            .checkpoint_dir(PathBuf::from("/checkpoints"))
            .token_budget_config(TokenBudget::new(200_000))
            .build()
            .unwrap();

        assert_eq!(config.model, "gpt-4");
        assert_eq!(config.max_iterations, 100);
        assert_eq!(config.max_tokens, 4096);
        assert_eq!(config.temperature, Some(0.7));
        assert_eq!(config.workspace_root, PathBuf::from("/workspace"));
        assert_eq!(config.token_budget, Some(50_000));
        assert_eq!(config.checkpoint_dir, Some(PathBuf::from("/checkpoints")));
        assert!(config.context_token_budget.is_some());
    }
}
