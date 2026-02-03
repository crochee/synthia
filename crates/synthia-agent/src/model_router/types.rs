use std::sync::Arc;

use async_trait::async_trait;
use rmcp::model::SamplingMessage;
use serde::{Deserialize, Serialize};
use synthia_provider::ModelProvider;

use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderType {
    Anthropic,
    OpenAI,
    OpenAICompatible,
    Custom,
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderType::Anthropic => write!(f, "anthropic"),
            ProviderType::OpenAI => write!(f, "openai"),
            ProviderType::OpenAICompatible => write!(f, "openai-compatible"),
            ProviderType::Custom => write!(f, "custom"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum ComplexityLevel {
    #[default]
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Default)]
pub struct ConversationMetrics {
    pub message_count: usize,
    pub total_tokens_estimate: usize,
    pub complexity: ComplexityLevel,
    pub tool_call_count: usize,
    pub consecutive_failures: usize,
}

#[derive(Debug, Clone)]
pub struct RoutingDecision {
    pub selected_model: String,
    pub provider_type: ProviderType,
    pub reasoning: String,
    pub matched_rules: Vec<String>,
    pub conversation_metrics: ConversationMetrics,
}

impl Default for RoutingDecision {
    fn default() -> Self {
        Self {
            selected_model: String::new(),
            provider_type: ProviderType::OpenAI,
            reasoning: String::new(),
            matched_rules: Vec::new(),
            conversation_metrics: ConversationMetrics::default(),
        }
    }
}

pub struct RoutingResult {
    pub provider: Arc<dyn ModelProvider>,
    pub config: ModelConfig,
    pub decision: RoutingDecision,
}

#[async_trait]
pub trait RoutingStrategy: Send + Sync {
    async fn route(
        &self,
        conversation: &[SamplingMessage],
        available_models: &[ModelConfig],
        decision: &mut RoutingDecision,
    ) -> Result<ModelConfig>;

    fn name(&self) -> &'static str;
}

#[async_trait]
pub trait ModelRouter: Send + Sync {
    async fn route(
        &self,
        conversation: &[SamplingMessage],
    ) -> Result<RoutingResult>;

    fn available_models(&self) -> &[ModelConfig];

    /// Returns the context window size for the currently selected model.
    /// Returns a fallback value if the context window is not known.
    fn context_window(&self) -> usize;
}

fn default_temperature() -> Option<f32> {
    Some(0.7)
}

fn default_max_tokens() -> u32 {
    4096
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub vision: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub context_window: Option<usize>,
    pub description: Option<String>,
    pub capabilities: Option<ModelCapabilities>,
    #[serde(default = "default_temperature")]
    pub temperature: Option<f32>,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

impl ModelInfo {
    pub fn with_name(name: &str) -> Self {
        Self {
            name: name.to_string(),
            api_key: None,
            base_url: None,
            context_window: None,
            description: None,
            capabilities: None,
            temperature: Some(0.7),
            max_tokens: 4096,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelConfig {
    Anthropic(ModelInfo),
    OpenAI(ModelInfo),
    OpenAICompatible {
        info: ModelInfo,
        base_url: String,
    },
    Custom {
        provider_type: String,
        info: ModelInfo,
    },
}

impl ModelConfig {
    pub fn model_info(&self) -> &ModelInfo {
        match self {
            ModelConfig::Anthropic(info) => info,
            ModelConfig::OpenAI(info) => info,
            ModelConfig::OpenAICompatible { info, .. } => info,
            ModelConfig::Custom { info, .. } => info,
        }
    }

    pub fn model_info_mut(&mut self) -> &mut ModelInfo {
        match self {
            ModelConfig::Anthropic(info) => info,
            ModelConfig::OpenAI(info) => info,
            ModelConfig::OpenAICompatible { info, .. } => info,
            ModelConfig::Custom { info, .. } => info,
        }
    }

    pub fn provider_type(&self) -> ProviderType {
        match self {
            ModelConfig::Anthropic(_) => ProviderType::Anthropic,
            ModelConfig::OpenAI(_) => ProviderType::OpenAI,
            ModelConfig::OpenAICompatible { .. } => {
                ProviderType::OpenAICompatible
            }
            ModelConfig::Custom { .. } => ProviderType::Custom,
        }
    }

    pub fn anthropic(name: &str) -> Self {
        ModelConfig::Anthropic(ModelInfo::with_name(name))
    }

    pub fn openai(name: &str) -> Self {
        ModelConfig::OpenAI(ModelInfo::with_name(name))
    }

    pub fn openai_compatible(name: &str, base_url: &str) -> Self {
        ModelConfig::OpenAICompatible {
            info: ModelInfo::with_name(name),
            base_url: base_url.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeywordMatch {
    Any,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    Gte,
    Lte,
    Eq,
}

#[derive(Debug, Clone)]
pub enum RoutingTrigger {
    Keywords {
        words: Vec<String>,
        match_type: KeywordMatch,
    },
    Complexity {
        level: ComplexityLevel,
        comparison: Comparison,
    },
    ConsecutiveTools {
        count: usize,
        comparison: Comparison,
    },
    ConsecutiveFailures {
        count: usize,
    },
    FirstTurn,
    MessageLength {
        min: Option<usize>,
        max: Option<usize>,
    },
    ToolFailure,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_type_display() {
        assert_eq!(format!("{}", ProviderType::Anthropic), "anthropic");
        assert_eq!(format!("{}", ProviderType::OpenAI), "openai");
        assert_eq!(
            format!("{}", ProviderType::OpenAICompatible),
            "openai-compatible"
        );
        assert_eq!(format!("{}", ProviderType::Custom), "custom");
    }

    #[test]
    fn test_complexity_level_default() {
        let level = ComplexityLevel::default();
        assert_eq!(level, ComplexityLevel::Low);
    }

    #[test]
    fn test_complexity_level_ordering() {
        assert!(ComplexityLevel::Low < ComplexityLevel::Medium);
        assert!(ComplexityLevel::Medium < ComplexityLevel::High);
    }

    #[test]
    fn test_conversation_metrics_default() {
        let metrics = ConversationMetrics::default();
        assert_eq!(metrics.message_count, 0);
        assert_eq!(metrics.total_tokens_estimate, 0);
        assert_eq!(metrics.complexity, ComplexityLevel::Low);
        assert_eq!(metrics.tool_call_count, 0);
        assert_eq!(metrics.consecutive_failures, 0);
    }

    #[test]
    fn test_routing_decision_default() {
        let decision = RoutingDecision::default();
        assert!(decision.selected_model.is_empty());
        assert_eq!(decision.provider_type, ProviderType::OpenAI);
        assert!(decision.reasoning.is_empty());
        assert!(decision.matched_rules.is_empty());
    }

    #[test]
    fn test_model_info_with_name() {
        let info = ModelInfo::with_name("claude-3");
        assert_eq!(info.name, "claude-3");
        assert!(info.api_key.is_none());
        assert!(info.base_url.is_none());
        assert!(info.context_window.is_none());
        assert!(info.description.is_none());
        assert_eq!(info.temperature, Some(0.7));
        assert_eq!(info.max_tokens, 4096);
    }

    #[test]
    fn test_model_config_anthropic() {
        let config = ModelConfig::anthropic("claude-3-opus");
        assert_eq!(config.provider_type(), ProviderType::Anthropic);
        assert_eq!(config.model_info().name, "claude-3-opus");
    }

    #[test]
    fn test_model_config_openai() {
        let config = ModelConfig::openai("gpt-4o");
        assert_eq!(config.provider_type(), ProviderType::OpenAI);
        assert_eq!(config.model_info().name, "gpt-4o");
    }

    #[test]
    fn test_model_config_openai_compatible() {
        let config = ModelConfig::openai_compatible(
            "custom-model",
            "https://api.example.com",
        );
        assert_eq!(config.provider_type(), ProviderType::OpenAICompatible);
        assert_eq!(config.model_info().name, "custom-model");
        if let ModelConfig::OpenAICompatible { base_url, .. } = config {
            assert_eq!(base_url, "https://api.example.com");
        } else {
            panic!("Expected OpenAICompatible");
        }
    }

    #[test]
    fn test_model_config_model_info_mut() {
        let mut config = ModelConfig::anthropic("claude-3");
        config.model_info_mut().name = "claude-3-sonnet".to_string();
        assert_eq!(config.model_info().name, "claude-3-sonnet");
    }

    #[test]
    fn test_keyword_match() {
        assert!(matches!(KeywordMatch::Any, KeywordMatch::Any));
        assert!(matches!(KeywordMatch::All, KeywordMatch::All));
    }

    #[test]
    fn test_comparison() {
        assert!(matches!(Comparison::Gte, Comparison::Gte));
        assert!(matches!(Comparison::Lte, Comparison::Lte));
        assert!(matches!(Comparison::Eq, Comparison::Eq));
    }

    #[test]
    fn test_routing_trigger_keywords() {
        let trigger = RoutingTrigger::Keywords {
            words: vec!["code".to_string(), "debug".to_string()],
            match_type: KeywordMatch::Any,
        };
        match trigger {
            RoutingTrigger::Keywords { words, match_type } => {
                assert_eq!(words.len(), 2);
                assert!(matches!(match_type, KeywordMatch::Any));
            }
            _ => panic!("Expected Keywords"),
        }
    }

    #[test]
    fn test_routing_trigger_complexity() {
        let trigger = RoutingTrigger::Complexity {
            level: ComplexityLevel::High,
            comparison: Comparison::Gte,
        };
        match trigger {
            RoutingTrigger::Complexity { level, comparison } => {
                assert!(matches!(level, ComplexityLevel::High));
                assert!(matches!(comparison, Comparison::Gte));
            }
            _ => panic!("Expected Complexity"),
        }
    }

    #[test]
    fn test_routing_trigger_consecutive_tools() {
        let trigger = RoutingTrigger::ConsecutiveTools {
            count: 5,
            comparison: Comparison::Gte,
        };
        match trigger {
            RoutingTrigger::ConsecutiveTools { count, comparison } => {
                assert_eq!(count, 5);
                assert!(matches!(comparison, Comparison::Gte));
            }
            _ => panic!("Expected ConsecutiveTools"),
        }
    }

    #[test]
    fn test_routing_trigger_first_turn() {
        let trigger = RoutingTrigger::FirstTurn;
        assert!(matches!(trigger, RoutingTrigger::FirstTurn));
    }

    #[test]
    fn test_routing_trigger_message_length() {
        let trigger = RoutingTrigger::MessageLength {
            min: Some(100),
            max: Some(1000),
        };
        match trigger {
            RoutingTrigger::MessageLength { min, max } => {
                assert_eq!(min, Some(100));
                assert_eq!(max, Some(1000));
            }
            _ => panic!("Expected MessageLength"),
        }
    }

    #[test]
    fn test_routing_trigger_tool_failure() {
        let trigger = RoutingTrigger::ToolFailure;
        assert!(matches!(trigger, RoutingTrigger::ToolFailure));
    }
}
