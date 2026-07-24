use std::sync::Arc;

use synthia_provider::ModelProvider;

use super::model::ModelConfig;

pub type Result<T> = anyhow::Result<T>;

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
