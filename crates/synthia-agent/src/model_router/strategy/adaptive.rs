use async_trait::async_trait;
use parking_lot::Mutex;
use rmcp::model::SamplingMessage;

use crate::{
    Result,
    model_router::{
        analyzer::ConversationAnalyzer,
        types::{ModelConfig, ProviderType, RoutingDecision, RoutingStrategy},
    },
};

pub struct AdaptiveStrategy {
    model_configs: Vec<ModelConfig>,
    state: Mutex<AdaptiveState>,
    analyzer: ConversationAnalyzer,
}

#[derive(Debug, Clone, Default)]
struct AdaptiveState {
    current_turn: usize,
    consecutive_tools: usize,
    consecutive_failures: usize,
}

impl AdaptiveStrategy {
    pub fn new(models: Vec<ModelConfig>) -> Self {
        Self {
            model_configs: models,
            state: Mutex::new(AdaptiveState::default()),
            analyzer: ConversationAnalyzer::new(),
        }
    }
}

impl AdaptiveStrategy {
    fn select_model_for_state(&self, state: &AdaptiveState) -> &ModelConfig {
        if state.consecutive_failures >= 2 {
            return self.fallback_model();
        }

        if state.consecutive_tools >= 3
            && let Some(model) = self
                .model_configs
                .iter()
                .find(|m| m.provider_type() == ProviderType::OpenAI)
        {
            return model;
        }

        if state.current_turn == 1
            && let Some(model) = self
                .model_configs
                .iter()
                .find(|m| m.provider_type() == ProviderType::Anthropic)
        {
            return model;
        }

        self.fallback_model()
    }

    fn fallback_model(&self) -> &ModelConfig {
        self.model_configs.first().unwrap_or(&self.model_configs[0])
    }
}

#[async_trait]
impl RoutingStrategy for AdaptiveStrategy {
    async fn route(
        &self,
        conversation: &[SamplingMessage],
        _models: &[ModelConfig],
        decision: &mut RoutingDecision,
    ) -> Result<ModelConfig> {
        let metrics = self.analyzer.analyze(conversation);

        let model = {
            let mut state = self.state.lock();
            state.current_turn = metrics.message_count;
            state.consecutive_tools = metrics.tool_call_count;
            state.consecutive_failures = metrics.consecutive_failures;
            self.select_model_for_state(&state)
        };

        decision.conversation_metrics = metrics;
        decision.selected_model = model.model_info().name.clone();
        decision.provider_type = model.provider_type();
        decision.reasoning =
            "Adaptive selection based on conversation state".to_string();

        Ok(model.clone())
    }

    fn name(&self) -> &'static str {
        "adaptive"
    }
}

#[cfg(test)]
mod tests {
    use rmcp::model::{
        RawTextContent,
        Role,
        SamplingContent,
        SamplingMessage,
        SamplingMessageContent,
    };

    use super::*;
    use crate::model_router::types::ProviderType;

    fn user_msg(text: &str) -> SamplingMessage {
        SamplingMessage {
            role: Role::User,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: text.to_string(),
                    meta: None,
                },
            )),
            meta: None,
        }
    }

    fn assistant_msg(text: &str) -> SamplingMessage {
        SamplingMessage {
            role: Role::Assistant,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: text.to_string(),
                    meta: None,
                },
            )),
            meta: None,
        }
    }

    // AdaptiveStrategy state machine tests.
    // select_model_for_state and fallback_model are private, so we test them
    // indirectly via the public route() method.

    #[test]
    fn test_adaptive_strategy_new() {
        let models = vec![ModelConfig::anthropic("claude-3")];
        let strategy = AdaptiveStrategy::new(models);
        assert_eq!(strategy.name(), "adaptive");
    }

    #[test]
    fn test_adaptive_strategy_route_first_turn_selects_anthropic() {
        let models = vec![
            ModelConfig::anthropic("claude-3"),
            ModelConfig::openai("gpt-4o"),
        ];
        let strategy = AdaptiveStrategy::new(models);

        // Single user message -> current_turn = 1
        let conversation = vec![user_msg("Hello")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &[], &mut decision));
        assert!(result.is_ok());
        assert_eq!(decision.provider_type, ProviderType::Anthropic);
        assert_eq!(decision.selected_model, "claude-3");
    }

    #[test]
    fn test_adaptive_strategy_route_many_tool_calls_selects_openai() {
        let models = vec![
            ModelConfig::anthropic("claude-3"),
            ModelConfig::openai("gpt-4o"),
        ];
        let strategy = AdaptiveStrategy::new(models);

        // 3+ assistant messages with tool_call after user -> consecutive_tools >= 3
        let conversation = vec![
            user_msg("Hello"),
            assistant_msg("I will use a tool_call to help"),
            assistant_msg("Another tool_call here"),
            assistant_msg("Third tool_call"),
        ];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &[], &mut decision));
        assert!(result.is_ok());
        // Should select OpenAI due to consecutive tools >= 3
        assert_eq!(decision.provider_type, ProviderType::OpenAI);
        assert_eq!(decision.selected_model, "gpt-4o");
    }

    #[test]
    fn test_adaptive_strategy_route_fallback_after_failures() {
        let models = vec![
            ModelConfig::anthropic("claude-3"),
            ModelConfig::openai("gpt-4o"),
        ];
        let strategy = AdaptiveStrategy::new(models);

        // Assistant messages with "error" -> consecutive_failures >= 2
        let conversation = vec![
            user_msg("Hello"),
            assistant_msg("error occurred"),
            assistant_msg("failed to process"),
        ];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &[], &mut decision));
        assert!(result.is_ok());
        // Falls back to first model (claude-3 / Anthropic) after >= 2 failures
        assert_eq!(decision.provider_type, ProviderType::Anthropic);
    }

    #[test]
    fn test_adaptive_strategy_route_mid_conversation_fallback() {
        let models = vec![
            ModelConfig::anthropic("claude-3"),
            ModelConfig::openai("gpt-4o"),
        ];
        let strategy = AdaptiveStrategy::new(models);

        // Conversation with 2+ turns, few tools, few failures -> fallback to first
        let conversation = vec![
            user_msg("Hello"),
            assistant_msg("I can help"),
            user_msg("Thanks"),
        ];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &[], &mut decision));
        assert!(result.is_ok());
        // Falls back to first model
        assert_eq!(decision.provider_type, ProviderType::Anthropic);
        assert_eq!(decision.selected_model, "claude-3");
    }

    #[test]
    fn test_adaptive_strategy_route_sets_conversation_metrics() {
        let models = vec![ModelConfig::anthropic("claude-3")];
        let strategy = AdaptiveStrategy::new(models);

        let conversation = vec![
            user_msg("Hello world this is a test message"),
            assistant_msg("I will use a tool_call"),
        ];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &[], &mut decision));
        assert!(result.is_ok());
        assert_eq!(decision.conversation_metrics.message_count, 2);
        assert!(decision.conversation_metrics.total_tokens_estimate > 0);
    }

    #[test]
    fn test_adaptive_strategy_route_reasoning_set() {
        let models = vec![
            ModelConfig::anthropic("claude-3"),
            ModelConfig::openai("gpt-4o"),
        ];
        let strategy = AdaptiveStrategy::new(models);

        let conversation = vec![user_msg("Hello")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &[], &mut decision));
        assert!(result.is_ok());
        assert!(!decision.reasoning.is_empty());
    }

    #[test]
    fn test_adaptive_strategy_route_empty_conversation() {
        let models = vec![ModelConfig::anthropic("claude-3")];
        let strategy = AdaptiveStrategy::new(models);

        let conversation: Vec<SamplingMessage> = vec![];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &[], &mut decision));
        assert!(result.is_ok());
        // Empty conversation -> message_count=0, not first turn, no tools,
        // no failures -> falls back to first model
        assert_eq!(decision.provider_type, ProviderType::Anthropic);
    }

    #[test]
    fn test_adaptive_strategy_route_single_model() {
        let models = vec![ModelConfig::openai("gpt-4o")];
        let strategy = AdaptiveStrategy::new(models);

        let conversation = vec![user_msg("Hello")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &[], &mut decision));
        assert!(result.is_ok());
        assert_eq!(decision.provider_type, ProviderType::OpenAI);
    }

    #[test]
    fn test_adaptive_strategy_route_state_accumulates_across_calls() {
        let models = vec![
            ModelConfig::anthropic("claude-3"),
            ModelConfig::openai("gpt-4o"),
        ];
        let strategy = AdaptiveStrategy::new(models);

        // First call: first turn -> Anthropic
        let conversation1 = vec![user_msg("Hello")];
        let mut decision1 =
            crate::model_router::types::RoutingDecision::default();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result1 =
            rt.block_on(strategy.route(&conversation1, &[], &mut decision1));
        assert!(result1.is_ok());
        assert_eq!(decision1.provider_type, ProviderType::Anthropic);

        // Second call: same conversation (still counts as first turn based on message count)
        let mut decision2 =
            crate::model_router::types::RoutingDecision::default();
        let result2 =
            rt.block_on(strategy.route(&conversation1, &[], &mut decision2));
        assert!(result2.is_ok());
        // Still first turn since message_count is still 1
        assert_eq!(decision2.provider_type, ProviderType::Anthropic);
    }
}
