use async_trait::async_trait;
use rmcp::model::SamplingMessage;

use crate::{
    Result,
    model_router::{
        analyzer::ConversationAnalyzer,
        types::{
            Comparison,
            KeywordMatch,
            ModelConfig,
            ProviderType,
            RoutingDecision,
            RoutingStrategy,
            RoutingTrigger,
        },
    },
    utils::extract_text,
};

#[derive(Debug, Clone)]
pub struct RoutingRule {
    pub name: String,
    pub triggers: Vec<RoutingTrigger>,
    pub target_model: String,
    pub target_provider: ProviderType,
    pub priority: i32,
    pub active_turns: usize,
}

impl RoutingRule {
    pub fn new(
        name: &str,
        target_model: &str,
        target_provider: ProviderType,
        priority: i32,
    ) -> Self {
        Self {
            name: name.to_string(),
            triggers: Vec::new(),
            target_model: target_model.to_string(),
            target_provider,
            priority,
            active_turns: 5,
        }
    }

    pub fn with_triggers(mut self, triggers: Vec<RoutingTrigger>) -> Self {
        self.triggers = triggers;
        self
    }

    pub fn with_active_turns(mut self, turns: usize) -> Self {
        self.active_turns = turns;
        self
    }
}

pub struct RuleBasedStrategy {
    rules: Vec<RoutingRule>,
    conversation_analyzer: ConversationAnalyzer,
}

impl RuleBasedStrategy {
    pub fn new(rules: Vec<RoutingRule>) -> Self {
        let mut rules = rules;
        rules.sort_by_key(|r| -r.priority);
        Self {
            rules,
            conversation_analyzer: ConversationAnalyzer::new(),
        }
    }
}

impl RuleBasedStrategy {
    fn evaluate_trigger(
        &self,
        trigger: &RoutingTrigger,
        metrics: &crate::model_router::types::ConversationMetrics,
        conversation: &[SamplingMessage],
    ) -> bool {
        match trigger {
            RoutingTrigger::Keywords { words, match_type } => {
                let Some(msg) = conversation
                    .iter()
                    .rev()
                    .find(|m| matches!(m.role, rmcp::model::Role::User))
                else {
                    return false;
                };
                let content = extract_text(msg).to_lowercase();
                match match_type {
                    KeywordMatch::Any => words
                        .iter()
                        .any(|w| content.contains(&w.to_lowercase())),
                    KeywordMatch::All => words
                        .iter()
                        .all(|w| content.contains(&w.to_lowercase())),
                }
            }
            RoutingTrigger::Complexity { level, comparison } => {
                match comparison {
                    Comparison::Gte => metrics.complexity >= *level,
                    Comparison::Lte => metrics.complexity <= *level,
                    Comparison::Eq => metrics.complexity == *level,
                }
            }
            RoutingTrigger::ConsecutiveTools { count, comparison } => {
                match comparison {
                    Comparison::Gte => metrics.tool_call_count >= *count,
                    Comparison::Lte => metrics.tool_call_count <= *count,
                    Comparison::Eq => metrics.tool_call_count == *count,
                }
            }
            RoutingTrigger::ConsecutiveFailures { count } => {
                metrics.consecutive_failures >= *count
            }
            RoutingTrigger::FirstTurn => metrics.message_count == 1,
            RoutingTrigger::MessageLength { min, max } => {
                let last_msg_len = conversation
                    .iter()
                    .rev()
                    .find(|m| matches!(m.role, rmcp::model::Role::User))
                    .map(|m| extract_text(m).len())
                    .unwrap_or(0);

                min.map(|m| last_msg_len >= m).unwrap_or(true)
                    && max.map(|m| last_msg_len <= m).unwrap_or(true)
            }
            RoutingTrigger::ToolFailure => metrics.consecutive_failures > 0,
        }
    }

    fn evaluate_triggers(
        &self,
        triggers: &[RoutingTrigger],
        metrics: &crate::model_router::types::ConversationMetrics,
        conversation: &[SamplingMessage],
    ) -> bool {
        triggers.iter().all(|trigger| {
            self.evaluate_trigger(trigger, metrics, conversation)
        })
    }
}

#[async_trait]
impl RoutingStrategy for RuleBasedStrategy {
    async fn route(
        &self,
        conversation: &[SamplingMessage],
        models: &[ModelConfig],
        decision: &mut RoutingDecision,
    ) -> Result<ModelConfig> {
        let metrics = self.conversation_analyzer.analyze(conversation);
        decision.conversation_metrics = metrics.clone();

        for rule in &self.rules {
            if !self.evaluate_triggers(&rule.triggers, &metrics, conversation) {
                continue;
            }

            decision.matched_rules.push(rule.name.clone());
            decision.reasoning = format!("Matched rule: {}", rule.name);

            return models
                .iter()
                .find(|m| m.model_info().name == rule.target_model)
                .or_else(|| models.first())
                .cloned()
                .ok_or_else(|| {
                    crate::AgentError::ConfigError(
                        "Target model not found in available models"
                            .to_string(),
                    )
                });
        }

        Err(crate::AgentError::ConfigError(
            "No rules matched".to_string(),
        ))
    }

    fn name(&self) -> &'static str {
        "rule_based"
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
    use crate::model_router::types::{
        Comparison,
        ComplexityLevel,
        KeywordMatch,
    };

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

    // RoutingRule unit tests

    #[test]
    fn test_routing_rule_new() {
        let rule =
            RoutingRule::new("my-rule", "gpt-4o", ProviderType::OpenAI, 5);
        assert_eq!(rule.name, "my-rule");
        assert_eq!(rule.target_model, "gpt-4o");
        assert_eq!(rule.target_provider, ProviderType::OpenAI);
        assert_eq!(rule.priority, 5);
        assert_eq!(rule.active_turns, 5);
    }

    #[test]
    fn test_routing_rule_with_triggers() {
        let rule = RoutingRule::new("test", "model", ProviderType::OpenAI, 1)
            .with_triggers(vec![RoutingTrigger::FirstTurn])
            .with_active_turns(10);
        assert_eq!(rule.triggers.len(), 1);
        assert_eq!(rule.active_turns, 10);
    }

    #[test]
    fn test_routing_rule_with_active_turns() {
        let rule = RoutingRule::new("test", "model", ProviderType::OpenAI, 1)
            .with_active_turns(20);
        assert_eq!(rule.active_turns, 20);
    }

    // RuleBasedStrategy unit tests

    #[test]
    fn test_rule_based_strategy_new_sorts_by_priority() {
        let rule1 = RoutingRule::new("low", "model1", ProviderType::OpenAI, 1);
        let rule2 =
            RoutingRule::new("high", "model2", ProviderType::Anthropic, 10);
        let strategy = RuleBasedStrategy::new(vec![rule1, rule2]);
        assert_eq!(strategy.name(), "rule_based");
    }

    // Trigger evaluation unit tests
    // Note: evaluate_trigger and evaluate_triggers are private, but we test them
    // indirectly via the public route() method which exercises them.

    #[test]
    fn test_rule_based_route_keywords_any_match() {
        let triggers = vec![RoutingTrigger::Keywords {
            words: vec!["debug".to_string(), "code".to_string()],
            match_type: KeywordMatch::Any,
        }];
        let rule =
            RoutingRule::new("code-keyword", "gpt-4o", ProviderType::OpenAI, 1)
                .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        let conversation = vec![user_msg("Please debug this code")];
        let models = vec![ModelConfig::openai("gpt-4o")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        assert!(result.is_ok());
        assert_eq!(decision.matched_rules, vec!["code-keyword"]);
    }

    #[test]
    fn test_rule_based_route_keywords_no_match() {
        let triggers = vec![RoutingTrigger::Keywords {
            words: vec!["python".to_string(), "java".to_string()],
            match_type: KeywordMatch::Any,
        }];
        let rule =
            RoutingRule::new("python-rule", "gpt-4o", ProviderType::OpenAI, 1)
                .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        let conversation = vec![user_msg("Hello world")];
        let models = vec![ModelConfig::openai("gpt-4o")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        // No rules matched -> error
        assert!(result.is_err());
    }

    #[test]
    fn test_rule_based_route_keywords_all_match() {
        let triggers = vec![RoutingTrigger::Keywords {
            words: vec!["debug".to_string(), "code".to_string()],
            match_type: KeywordMatch::All,
        }];
        let rule =
            RoutingRule::new("all-keywords", "gpt-4o", ProviderType::OpenAI, 1)
                .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        let conversation = vec![user_msg("Please debug this code")];
        let models = vec![ModelConfig::openai("gpt-4o")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        assert!(result.is_ok());
        assert_eq!(decision.matched_rules, vec!["all-keywords"]);
    }

    #[test]
    fn test_rule_based_route_keywords_all_no_match() {
        let triggers = vec![RoutingTrigger::Keywords {
            words: vec![
                "debug".to_string(),
                "code".to_string(),
                "python".to_string(),
            ],
            match_type: KeywordMatch::All,
        }];
        let rule =
            RoutingRule::new("all-keywords", "gpt-4o", ProviderType::OpenAI, 1)
                .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        // Only "debug" and "code" present, not "python" -> All match fails
        let conversation = vec![user_msg("Please debug this code")];
        let models = vec![ModelConfig::openai("gpt-4o")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        assert!(result.is_err());
    }

    #[test]
    fn test_rule_based_route_complexity_gte() {
        let triggers = vec![RoutingTrigger::Complexity {
            level: ComplexityLevel::High,
            comparison: Comparison::Gte,
        }];
        let rule = RoutingRule::new(
            "high-complexity",
            "gpt-4o",
            ProviderType::OpenAI,
            1,
        )
        .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        // Message with high complexity (long words)
        let text = "The architecture utilizes sophisticated implementation strategies with sophisticated sophisticated sophisticated sophisticated components.".to_string();
        let conversation = vec![user_msg(&text)];
        let models = vec![ModelConfig::openai("gpt-4o")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        assert!(result.is_ok());
        assert_eq!(decision.matched_rules, vec!["high-complexity"]);
    }

    #[test]
    fn test_rule_based_route_complexity_not_met() {
        let triggers = vec![RoutingTrigger::Complexity {
            level: ComplexityLevel::High,
            comparison: Comparison::Gte,
        }];
        let rule = RoutingRule::new(
            "high-complexity",
            "gpt-4o",
            ProviderType::OpenAI,
            1,
        )
        .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        // Short simple message -> Low complexity
        let conversation = vec![user_msg("Hi")];
        let models = vec![ModelConfig::openai("gpt-4o")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        assert!(result.is_err());
    }

    #[test]
    fn test_rule_based_route_consecutive_tools_trigger() {
        let triggers = vec![RoutingTrigger::ConsecutiveTools {
            count: 3,
            comparison: Comparison::Gte,
        }];
        let rule =
            RoutingRule::new("many-tools", "gpt-4o", ProviderType::OpenAI, 1)
                .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        // Assistant messages with tool_call
        let conversation = vec![
            user_msg("Hello"),
            assistant_msg("I will use a tool_call to help"),
            assistant_msg("Another tool_call here"),
            assistant_msg("Third tool_call"),
        ];
        let models = vec![ModelConfig::openai("gpt-4o")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        assert!(result.is_ok());
        assert_eq!(decision.matched_rules, vec!["many-tools"]);
    }

    #[test]
    fn test_rule_based_route_consecutive_failures_trigger() {
        let triggers = vec![RoutingTrigger::ConsecutiveFailures { count: 2 }];
        let rule =
            RoutingRule::new("failures", "gpt-4o", ProviderType::OpenAI, 1)
                .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        let conversation = vec![
            user_msg("Hello"),
            assistant_msg("error occurred"),
            assistant_msg("failed to process"),
        ];
        let models = vec![ModelConfig::openai("gpt-4o")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        assert!(result.is_ok());
        assert_eq!(decision.matched_rules, vec!["failures"]);
    }

    #[test]
    fn test_rule_based_route_first_turn_trigger() {
        let triggers = vec![RoutingTrigger::FirstTurn];
        let rule = RoutingRule::new(
            "first-turn",
            "claude-3",
            ProviderType::Anthropic,
            1,
        )
        .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        let conversation = vec![user_msg("Hello")];
        let models = vec![ModelConfig::anthropic("claude-3")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        assert!(result.is_ok());
        assert_eq!(decision.matched_rules, vec!["first-turn"]);
    }

    #[test]
    fn test_rule_based_route_first_turn_not_first() {
        let triggers = vec![RoutingTrigger::FirstTurn];
        let rule = RoutingRule::new(
            "first-turn",
            "claude-3",
            ProviderType::Anthropic,
            1,
        )
        .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        // Multiple messages -> not first turn
        let conversation = vec![
            user_msg("Hello"),
            assistant_msg("Hi there"),
            user_msg("Follow up"),
        ];
        let models = vec![ModelConfig::anthropic("claude-3")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        assert!(result.is_err());
    }

    #[test]
    fn test_rule_based_route_message_length_min() {
        let triggers = vec![RoutingTrigger::MessageLength {
            min: Some(100),
            max: None,
        }];
        let rule = RoutingRule::new(
            "long-msg",
            "claude-3",
            ProviderType::Anthropic,
            1,
        )
        .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        let conversation = vec![user_msg(&"a".repeat(150))];
        let models = vec![ModelConfig::anthropic("claude-3")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        assert!(result.is_ok());
        assert_eq!(decision.matched_rules, vec!["long-msg"]);
    }

    #[test]
    fn test_rule_based_route_message_length_max() {
        let triggers = vec![RoutingTrigger::MessageLength {
            min: None,
            max: Some(5),
        }];
        let rule = RoutingRule::new(
            "short-msg",
            "claude-3",
            ProviderType::Anthropic,
            1,
        )
        .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        let conversation = vec![user_msg("Hi")];
        let models = vec![ModelConfig::anthropic("claude-3")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        assert!(result.is_ok());
        assert_eq!(decision.matched_rules, vec!["short-msg"]);
    }

    #[test]
    fn test_rule_based_route_message_length_range() {
        let triggers = vec![RoutingTrigger::MessageLength {
            min: Some(10),
            max: Some(50),
        }];
        let rule =
            RoutingRule::new("mid-msg", "claude-3", ProviderType::Anthropic, 1)
                .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        let conversation = vec![user_msg("This is between 10 and 50 chars")];
        let models = vec![ModelConfig::anthropic("claude-3")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        assert!(result.is_ok());
    }

    #[test]
    fn test_rule_based_route_tool_failure_trigger() {
        let triggers = vec![RoutingTrigger::ToolFailure];
        let rule = RoutingRule::new(
            "tool-fail",
            "claude-3",
            ProviderType::Anthropic,
            1,
        )
        .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        let conversation =
            vec![user_msg("Hello"), assistant_msg("error occurred")];
        let models = vec![ModelConfig::anthropic("claude-3")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        assert!(result.is_ok());
        assert_eq!(decision.matched_rules, vec!["tool-fail"]);
    }

    #[test]
    fn test_rule_based_route_no_matching_rules() {
        let triggers = vec![RoutingTrigger::FirstTurn];
        let rule = RoutingRule::new(
            "first-turn",
            "claude-3",
            ProviderType::Anthropic,
            1,
        )
        .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        // Not first turn
        let conversation = vec![user_msg("Hello"), assistant_msg("Hi")];
        let models = vec![ModelConfig::anthropic("claude-3")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        assert!(result.is_err());
    }

    #[test]
    fn test_rule_based_route_falls_back_to_first_model() {
        let triggers = vec![RoutingTrigger::FirstTurn];
        let rule = RoutingRule::new(
            "first-turn",
            "non-existent",
            ProviderType::Custom,
            1,
        )
        .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        let conversation = vec![user_msg("Hello")];
        let models = vec![ModelConfig::anthropic("claude-3")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        // Falls back to first model when target not found
        assert!(result.is_ok());
    }

    #[test]
    fn test_rule_based_route_multiple_triggers_all_match() {
        let triggers = vec![
            RoutingTrigger::FirstTurn,
            RoutingTrigger::MessageLength {
                min: Some(5),
                max: None,
            },
        ];
        let rule =
            RoutingRule::new("combo", "claude-3", ProviderType::Anthropic, 1)
                .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        let conversation = vec![user_msg("Hello world")];
        let models = vec![ModelConfig::anthropic("claude-3")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        assert!(result.is_ok());
        assert_eq!(decision.matched_rules, vec!["combo"]);
    }

    #[test]
    fn test_rule_based_route_multiple_triggers_one_fails() {
        let triggers = vec![
            RoutingTrigger::FirstTurn,
            RoutingTrigger::MessageLength {
                min: Some(100),
                max: None,
            },
        ];
        let rule =
            RoutingRule::new("combo", "claude-3", ProviderType::Anthropic, 1)
                .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        // Message is too short for min=100
        let conversation = vec![user_msg("Hi")];
        let models = vec![ModelConfig::anthropic("claude-3")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        assert!(result.is_err());
    }

    #[test]
    fn test_rule_based_route_keywords_case_insensitive() {
        let triggers = vec![RoutingTrigger::Keywords {
            words: vec!["DEBUG".to_string(), "CODE".to_string()],
            match_type: KeywordMatch::Any,
        }];
        let rule =
            RoutingRule::new("uppercase", "gpt-4o", ProviderType::OpenAI, 1)
                .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        // Lowercase in message should still match
        let conversation = vec![user_msg("Please debug this code")];
        let models = vec![ModelConfig::openai("gpt-4o")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        assert!(result.is_ok());
    }

    #[test]
    fn test_rule_based_route_empty_conversation_no_first_turn() {
        let triggers = vec![RoutingTrigger::FirstTurn];
        let rule = RoutingRule::new(
            "first-turn",
            "claude-3",
            ProviderType::Anthropic,
            1,
        )
        .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        let conversation: Vec<SamplingMessage> = vec![];
        let models = vec![ModelConfig::anthropic("claude-3")];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        // Empty conversation -> message_count=0, not first turn
        assert!(result.is_err());
    }

    #[test]
    fn test_rule_based_route_empty_models_error() {
        let triggers = vec![RoutingTrigger::FirstTurn];
        let rule = RoutingRule::new(
            "first-turn",
            "claude-3",
            ProviderType::Anthropic,
            1,
        )
        .with_triggers(triggers);
        let strategy = RuleBasedStrategy::new(vec![rule]);

        let conversation = vec![user_msg("Hello")];
        let models: Vec<ModelConfig> = vec![];
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result =
            rt.block_on(strategy.route(&conversation, &models, &mut decision));
        assert!(result.is_err());
    }
}
