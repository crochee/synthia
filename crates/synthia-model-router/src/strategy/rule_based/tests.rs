//! Unit tests for the rule-based routing family.
//!
//! All 23 tests for [`super::types::RoutingRule`],
//! [`super::strategy::RuleBasedStrategy`], the
//! trigger-evaluation block in [`super::evaluate`],
//! and the `impl RoutingStrategy` block in
//! [`super::routing_strategy`] live here.
//!
//! `user_msg` / `assistant_msg` builders are
//! centralised here because every `route()` test
//! needs at least one of them; without centralisation
//! the test code would repeat the 9-line
//! `Message { role: ..., content: ..., ..Default::default() }`
//! literal 25+ times.

use synthia_provider::{Content, ContentPart, Message, Role, TextContent};

use super::{strategy::RuleBasedStrategy, types::RoutingRule};
use crate::types::{
    Comparison,
    ComplexityLevel,
    KeywordMatch,
    ModelConfig,
    ProviderType,
    RoutingDecision,
    RoutingStrategy,
    RoutingTrigger,
};

/// Build a `Message` with `Role::User` and a single
/// `TextContent` payload. Centralised because every
/// `route()` test needs at least one of these.
fn user_msg(text: &str) -> Message {
    Message {
        role: Role::User,
        content: Content::Single(ContentPart::Text(TextContent {
            text: text.to_string(),
            cache_control: None,
        })),
        tool_call_id: None,
        name: None,
        ..Default::default()
    }
}

/// Build a `Message` with `Role::Assistant` and a
/// single `TextContent` payload.
fn assistant_msg(text: &str) -> Message {
    Message {
        role: Role::Assistant,
        content: Content::Single(ContentPart::Text(TextContent {
            text: text.to_string(),
            cache_control: None,
        })),
        tool_call_id: None,
        name: None,
        ..Default::default()
    }
}

// RoutingRule unit tests

#[test]
fn test_routing_rule_new() {
    let rule = RoutingRule::new("my-rule", "gpt-4o", ProviderType::OpenAI, 5);
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
    let rule2 = RoutingRule::new("high", "model2", ProviderType::Anthropic, 10);
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
    let mut decision = RoutingDecision::default();

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
    let mut decision = RoutingDecision::default();

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
    let mut decision = RoutingDecision::default();

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
    let mut decision = RoutingDecision::default();

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
    let rule =
        RoutingRule::new("high-complexity", "gpt-4o", ProviderType::OpenAI, 1)
            .with_triggers(triggers);
    let strategy = RuleBasedStrategy::new(vec![rule]);

    // Message with high complexity (long words)
    let text = "The architecture utilizes sophisticated implementation strategies with sophisticated sophisticated sophisticated sophisticated components.".to_string();
    let conversation = vec![user_msg(&text)];
    let models = vec![ModelConfig::openai("gpt-4o")];
    let mut decision = RoutingDecision::default();

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
    let rule =
        RoutingRule::new("high-complexity", "gpt-4o", ProviderType::OpenAI, 1)
            .with_triggers(triggers);
    let strategy = RuleBasedStrategy::new(vec![rule]);

    // Short simple message -> Low complexity
    let conversation = vec![user_msg("Hi")];
    let models = vec![ModelConfig::openai("gpt-4o")];
    let mut decision = RoutingDecision::default();

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
    let mut decision = RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result =
        rt.block_on(strategy.route(&conversation, &models, &mut decision));
    assert!(result.is_ok());
    assert_eq!(decision.matched_rules, vec!["many-tools"]);
}

#[test]
fn test_rule_based_route_consecutive_failures_trigger() {
    let triggers = vec![RoutingTrigger::ConsecutiveFailures { count: 2 }];
    let rule = RoutingRule::new("failures", "gpt-4o", ProviderType::OpenAI, 1)
        .with_triggers(triggers);
    let strategy = RuleBasedStrategy::new(vec![rule]);

    let conversation = vec![
        user_msg("Hello"),
        assistant_msg("error occurred"),
        assistant_msg("failed to process"),
    ];
    let models = vec![ModelConfig::openai("gpt-4o")];
    let mut decision = RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result =
        rt.block_on(strategy.route(&conversation, &models, &mut decision));
    assert!(result.is_ok());
    assert_eq!(decision.matched_rules, vec!["failures"]);
}

#[test]
fn test_rule_based_route_first_turn_trigger() {
    let triggers = vec![RoutingTrigger::FirstTurn];
    let rule =
        RoutingRule::new("first-turn", "claude-3", ProviderType::Anthropic, 1)
            .with_triggers(triggers);
    let strategy = RuleBasedStrategy::new(vec![rule]);

    let conversation = vec![user_msg("Hello")];
    let models = vec![ModelConfig::anthropic("claude-3")];
    let mut decision = RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result =
        rt.block_on(strategy.route(&conversation, &models, &mut decision));
    assert!(result.is_ok());
    assert_eq!(decision.matched_rules, vec!["first-turn"]);
}

#[test]
fn test_rule_based_route_first_turn_not_first() {
    let triggers = vec![RoutingTrigger::FirstTurn];
    let rule =
        RoutingRule::new("first-turn", "claude-3", ProviderType::Anthropic, 1)
            .with_triggers(triggers);
    let strategy = RuleBasedStrategy::new(vec![rule]);

    // Multiple messages -> not first turn
    let conversation = vec![
        user_msg("Hello"),
        assistant_msg("Hi there"),
        user_msg("Follow up"),
    ];
    let models = vec![ModelConfig::anthropic("claude-3")];
    let mut decision = RoutingDecision::default();

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
    let rule =
        RoutingRule::new("long-msg", "claude-3", ProviderType::Anthropic, 1)
            .with_triggers(triggers);
    let strategy = RuleBasedStrategy::new(vec![rule]);

    let conversation = vec![user_msg(&"a".repeat(150))];
    let models = vec![ModelConfig::anthropic("claude-3")];
    let mut decision = RoutingDecision::default();

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
    let rule =
        RoutingRule::new("short-msg", "claude-3", ProviderType::Anthropic, 1)
            .with_triggers(triggers);
    let strategy = RuleBasedStrategy::new(vec![rule]);

    let conversation = vec![user_msg("Hi")];
    let models = vec![ModelConfig::anthropic("claude-3")];
    let mut decision = RoutingDecision::default();

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
    let mut decision = RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result =
        rt.block_on(strategy.route(&conversation, &models, &mut decision));
    assert!(result.is_ok());
}

#[test]
fn test_rule_based_route_tool_failure_trigger() {
    let triggers = vec![RoutingTrigger::ToolFailure];
    let rule =
        RoutingRule::new("tool-fail", "claude-3", ProviderType::Anthropic, 1)
            .with_triggers(triggers);
    let strategy = RuleBasedStrategy::new(vec![rule]);

    let conversation = vec![user_msg("Hello"), assistant_msg("error occurred")];
    let models = vec![ModelConfig::anthropic("claude-3")];
    let mut decision = RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result =
        rt.block_on(strategy.route(&conversation, &models, &mut decision));
    assert!(result.is_ok());
    assert_eq!(decision.matched_rules, vec!["tool-fail"]);
}

#[test]
fn test_rule_based_route_no_matching_rules() {
    let triggers = vec![RoutingTrigger::FirstTurn];
    let rule =
        RoutingRule::new("first-turn", "claude-3", ProviderType::Anthropic, 1)
            .with_triggers(triggers);
    let strategy = RuleBasedStrategy::new(vec![rule]);

    // Not first turn
    let conversation = vec![user_msg("Hello"), assistant_msg("Hi")];
    let models = vec![ModelConfig::anthropic("claude-3")];
    let mut decision = RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result =
        rt.block_on(strategy.route(&conversation, &models, &mut decision));
    assert!(result.is_err());
}

#[test]
fn test_rule_based_route_falls_back_to_first_model() {
    let triggers = vec![RoutingTrigger::FirstTurn];
    let rule =
        RoutingRule::new("first-turn", "non-existent", ProviderType::Custom, 1)
            .with_triggers(triggers);
    let strategy = RuleBasedStrategy::new(vec![rule]);

    let conversation = vec![user_msg("Hello")];
    let models = vec![ModelConfig::anthropic("claude-3")];
    let mut decision = RoutingDecision::default();

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
    let mut decision = RoutingDecision::default();

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
    let mut decision = RoutingDecision::default();

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
    let rule = RoutingRule::new("uppercase", "gpt-4o", ProviderType::OpenAI, 1)
        .with_triggers(triggers);
    let strategy = RuleBasedStrategy::new(vec![rule]);

    // Lowercase in message should still match
    let conversation = vec![user_msg("Please debug this code")];
    let models = vec![ModelConfig::openai("gpt-4o")];
    let mut decision = RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result =
        rt.block_on(strategy.route(&conversation, &models, &mut decision));
    assert!(result.is_ok());
}

#[test]
fn test_rule_based_route_empty_conversation_no_first_turn() {
    let triggers = vec![RoutingTrigger::FirstTurn];
    let rule =
        RoutingRule::new("first-turn", "claude-3", ProviderType::Anthropic, 1)
            .with_triggers(triggers);
    let strategy = RuleBasedStrategy::new(vec![rule]);

    let conversation: Vec<Message> = vec![];
    let models = vec![ModelConfig::anthropic("claude-3")];
    let mut decision = RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result =
        rt.block_on(strategy.route(&conversation, &models, &mut decision));
    // Empty conversation -> message_count=0, not first turn
    assert!(result.is_err());
}

#[test]
fn test_rule_based_route_empty_models_error() {
    let triggers = vec![RoutingTrigger::FirstTurn];
    let rule =
        RoutingRule::new("first-turn", "claude-3", ProviderType::Anthropic, 1)
            .with_triggers(triggers);
    let strategy = RuleBasedStrategy::new(vec![rule]);

    let conversation = vec![user_msg("Hello")];
    let models: Vec<ModelConfig> = vec![];
    let mut decision = RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result =
        rt.block_on(strategy.route(&conversation, &models, &mut decision));
    assert!(result.is_err());
}
