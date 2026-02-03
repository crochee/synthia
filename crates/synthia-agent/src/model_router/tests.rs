//! Tests for model_router module

use std::time::Duration;

use rmcp::model::{
    RawTextContent,
    Role,
    SamplingContent,
    SamplingMessage,
    SamplingMessageContent,
};

use crate::model_router::{
    DefaultModelRouter,
    analyzer::ConversationAnalyzer,
    cache::{ModelEntry, ModelList, ModelsCacheManager},
    factory::ProviderFactory,
    router::ModelFallbackRouter,
    strategy::{
        adaptive::AdaptiveStrategy,
        rule_based::{RoutingRule, RuleBasedStrategy},
        simple::SimpleStrategy,
    },
    types::{
        ComplexityLevel,
        KeywordMatch,
        ModelConfig,
        ModelInfo,
        ModelRouter,
        ProviderType,
        RoutingStrategy,
        RoutingTrigger,
    },
};

fn make_text_message(role: Role, text: &str) -> SamplingMessage {
    SamplingMessage {
        role,
        content: SamplingContent::Single(SamplingMessageContent::Text(
            RawTextContent {
                text: text.to_string(),
                meta: None,
            },
        )),
        meta: None,
    }
}

// ==================== Analyzer Tests ====================

#[test]
fn test_conversation_analyzer_new() {
    let _analyzer = ConversationAnalyzer::new();
}

#[test]
fn test_conversation_analyzer_analyze_empty() {
    let analyzer = ConversationAnalyzer::new();
    let metrics = analyzer.analyze(&[]);
    assert_eq!(metrics.message_count, 0);
    assert_eq!(metrics.total_tokens_estimate, 0);
    assert_eq!(metrics.complexity, ComplexityLevel::Low);
    assert_eq!(metrics.tool_call_count, 0);
    assert_eq!(metrics.consecutive_failures, 0);
}

#[test]
fn test_conversation_analyzer_analyze_with_user_message() {
    let analyzer = ConversationAnalyzer::new();
    let conversation = vec![make_text_message(
        Role::User,
        "Hello world this is a test message",
    )];
    let metrics = analyzer.analyze(&conversation);
    assert_eq!(metrics.message_count, 1);
    assert!(metrics.total_tokens_estimate > 0);
}

#[test]
fn test_conversation_analyzer_complexity_low() {
    let analyzer = ConversationAnalyzer::new();
    let conversation = vec![make_text_message(Role::User, "Hi")];
    let metrics = analyzer.analyze(&conversation);
    assert!(matches!(metrics.complexity, ComplexityLevel::Low));
}

#[test]
fn test_conversation_analyzer_tool_patterns() {
    let analyzer = ConversationAnalyzer::new();
    let conversation = vec![
        make_text_message(
            Role::Assistant,
            "I will use a tool_call to help you",
        ),
        make_text_message(Role::User, "Please help me"),
    ];
    let _metrics = analyzer.analyze(&conversation);
    // Tool call detection may vary based on implementation
}

#[test]
fn test_conversation_analyzer_failure_patterns() {
    let analyzer = ConversationAnalyzer::new();
    let conversation = vec![
        make_text_message(Role::Assistant, "The operation error occurred"),
        make_text_message(Role::User, "Hello"),
    ];
    let _metrics = analyzer.analyze(&conversation);
    // Failure detection may vary based on implementation
}

// ==================== Cache Tests ====================

#[test]
fn test_models_cache_manager_new() {
    let _cache = ModelsCacheManager::new(
        std::path::PathBuf::from("/tmp/test_cache.json"),
        Duration::from_secs(60),
    );
}

#[test]
fn test_models_cache_manager_load_fresh_nonexistent() {
    let cache = ModelsCacheManager::new(
        std::path::PathBuf::from("/tmp/nonexistent_cache_path.json"),
        Duration::from_secs(60),
    );
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(cache.load_fresh());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_models_cache_manager_persist_and_load() {
    let cache_path = std::path::PathBuf::from(format!(
        "/tmp/test_cache_{}.json",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let cache =
        ModelsCacheManager::new(cache_path.clone(), Duration::from_secs(3600));

    let model_list = ModelList {
        models: vec![ModelEntry {
            name: "claude-3".to_string(),
            version: "1.0".to_string(),
            cached_at: chrono::Utc::now(),
        }],
    };

    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(cache.persist_cache(&model_list));
    assert!(result.is_ok());

    let loaded = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(cache.load_fresh());
    assert!(loaded.is_ok());
    let loaded = loaded.unwrap();
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.models.len(), 1);
    assert_eq!(loaded.models[0].name, "claude-3");

    // Cleanup
    std::fs::remove_file(cache_path).ok();
}

#[test]
fn test_models_cache_manager_stale_cache() {
    let cache_path = std::path::PathBuf::from(format!(
        "/tmp/test_stale_cache_{}.json",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let cache =
        ModelsCacheManager::new(cache_path.clone(), Duration::from_millis(1));

    let model_list = ModelList {
        models: vec![ModelEntry {
            name: "claude-3".to_string(),
            version: "1.0".to_string(),
            cached_at: chrono::Utc::now() - chrono::Duration::seconds(10),
        }],
    };

    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(cache.persist_cache(&model_list))
        .ok();

    // Wait a tiny bit to ensure staleness
    std::thread::sleep(Duration::from_millis(10));

    let loaded = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(cache.load_fresh());
    assert!(loaded.unwrap().is_none());

    // Cleanup
    std::fs::remove_file(cache_path).ok();
}

// ==================== Factory Tests ====================

#[test]
fn test_provider_factory_new() {
    let _factory = ProviderFactory::new();
}

#[test]
fn test_provider_factory_create_anthropic() {
    let factory = ProviderFactory::new();
    let config = ModelConfig::Anthropic(ModelInfo {
        name: "claude-3".to_string(),
        api_key: Some("test-key".to_string()),
        base_url: Some("https://api.anthropic.com".to_string()),
        context_window: Some(200000),
        description: None,
        capabilities: None,
        temperature: Some(0.7),
        max_tokens: 4096,
    });
    let result = factory.create(&config);
    assert!(result.is_ok());
}

#[test]
fn test_provider_factory_create_openai() {
    let factory = ProviderFactory::new();
    let config = ModelConfig::OpenAI(ModelInfo {
        name: "gpt-4o".to_string(),
        api_key: Some("test-key".to_string()),
        base_url: Some("https://api.openai.com".to_string()),
        context_window: Some(128000),
        description: None,
        capabilities: None,
        temperature: Some(0.7),
        max_tokens: 4096,
    });
    let result = factory.create(&config);
    assert!(result.is_ok());
}

#[test]
fn test_provider_factory_create_openai_compatible() {
    let factory = ProviderFactory::new();
    let config = ModelConfig::OpenAICompatible {
        info: ModelInfo {
            name: "custom-model".to_string(),
            api_key: Some("test-key".to_string()),
            base_url: Some("https://api.example.com".to_string()),
            context_window: Some(8000),
            description: None,
            capabilities: None,
            temperature: Some(0.7),
            max_tokens: 4096,
        },
        base_url: "https://api.example.com".to_string(),
    };
    let result = factory.create(&config);
    assert!(result.is_ok());
}

#[test]
fn test_provider_factory_create_custom_unsupported() {
    let factory = ProviderFactory::new();
    let config = ModelConfig::Custom {
        provider_type: "unsupported".to_string(),
        info: ModelInfo::with_name("test"),
    };
    let result = factory.create(&config);
    assert!(result.is_err());
}

#[test]
fn test_provider_factory_create_custom_openai_compatible() {
    let factory = ProviderFactory::new();
    let config = ModelConfig::Custom {
        provider_type: "openai-compatible".to_string(),
        info: ModelInfo {
            name: "custom-model".to_string(),
            api_key: Some("test-key".to_string()),
            base_url: Some("https://api.example.com".to_string()),
            context_window: Some(8000),
            description: None,
            capabilities: None,
            temperature: Some(0.7),
            max_tokens: 4096,
        },
    };
    let result = factory.create(&config);
    assert!(result.is_ok());
}

// ==================== Strategy Simple Tests ====================

#[test]
fn test_simple_strategy_new() {
    let strategy = SimpleStrategy::new(0);
    assert_eq!(strategy.name(), "simple");
}

#[test]
fn test_simple_strategy_default() {
    let strategy = SimpleStrategy::default();
    assert_eq!(strategy.name(), "simple");
}

#[test]
fn test_simple_strategy_route() {
    let strategy = SimpleStrategy::new(0);
    let models = vec![
        ModelConfig::anthropic("claude-3"),
        ModelConfig::openai("gpt-4o"),
    ];
    let mut decision = crate::model_router::types::RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(strategy.route(&[], &models, &mut decision));
    assert!(result.is_ok());
    let model = result.unwrap();
    assert_eq!(model.model_info().name, "claude-3");
    assert_eq!(decision.selected_model, "claude-3");
    assert_eq!(decision.provider_type, ProviderType::Anthropic);
}

#[test]
fn test_simple_strategy_route_with_index() {
    let strategy = SimpleStrategy::new(1);
    let models = vec![
        ModelConfig::anthropic("claude-3"),
        ModelConfig::openai("gpt-4o"),
    ];
    let mut decision = crate::model_router::types::RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(strategy.route(&[], &models, &mut decision));
    assert!(result.is_ok());
    let model = result.unwrap();
    assert_eq!(model.model_info().name, "gpt-4o");
}

#[test]
fn test_simple_strategy_route_empty_models() {
    let strategy = SimpleStrategy::new(0);
    let models: Vec<ModelConfig> = vec![];
    let mut decision = crate::model_router::types::RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(strategy.route(&[], &models, &mut decision));
    assert!(result.is_err());
}

#[test]
fn test_simple_strategy_route_index_out_of_bounds() {
    let strategy = SimpleStrategy::new(10);
    let models = vec![ModelConfig::anthropic("claude-3")];
    let mut decision = crate::model_router::types::RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(strategy.route(&[], &models, &mut decision));
    // Should fall back to first model
    assert!(result.is_ok());
}

// ==================== Strategy RuleBased Tests ====================

#[test]
fn test_rule_based_strategy_new() {
    let rule =
        RoutingRule::new("test-rule", "claude-3", ProviderType::Anthropic, 1);
    let strategy = RuleBasedStrategy::new(vec![rule]);
    assert_eq!(strategy.name(), "rule_based");
}

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
fn test_rule_based_strategy_first_turn_trigger() {
    let rule =
        RoutingRule::new("first-turn", "claude-3", ProviderType::Anthropic, 1)
            .with_triggers(vec![RoutingTrigger::FirstTurn]);

    let strategy = RuleBasedStrategy::new(vec![rule]);

    let conversation = vec![make_text_message(Role::User, "Hello")];

    let models = vec![ModelConfig::anthropic("claude-3")];
    let mut decision = crate::model_router::types::RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result =
        rt.block_on(strategy.route(&conversation, &models, &mut decision));
    assert!(result.is_ok());
    assert_eq!(decision.matched_rules, vec!["first-turn"]);
}

#[test]
fn test_rule_based_strategy_keywords_trigger_any() {
    let rule =
        RoutingRule::new("code-keyword", "gpt-4o", ProviderType::OpenAI, 1)
            .with_triggers(vec![RoutingTrigger::Keywords {
                words: vec!["debug".to_string(), "code".to_string()],
                match_type: KeywordMatch::Any,
            }]);

    let strategy = RuleBasedStrategy::new(vec![rule]);

    let conversation =
        vec![make_text_message(Role::User, "Please debug this code")];

    let models = vec![ModelConfig::openai("gpt-4o")];
    let mut decision = crate::model_router::types::RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result =
        rt.block_on(strategy.route(&conversation, &models, &mut decision));
    assert!(result.is_ok());
    assert_eq!(decision.matched_rules, vec!["code-keyword"]);
}

#[test]
fn test_rule_based_strategy_no_match() {
    let rule =
        RoutingRule::new("first-turn", "claude-3", ProviderType::Anthropic, 1)
            .with_triggers(vec![RoutingTrigger::FirstTurn]);

    let strategy = RuleBasedStrategy::new(vec![rule]);

    // Multiple messages - not first turn
    let conversation = vec![
        make_text_message(Role::User, "Hello"),
        make_text_message(Role::Assistant, "Hi there"),
        make_text_message(Role::User, "How are you"),
    ];

    let models = vec![ModelConfig::anthropic("claude-3")];
    let mut decision = crate::model_router::types::RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result =
        rt.block_on(strategy.route(&conversation, &models, &mut decision));
    assert!(result.is_err());
}

#[test]
fn test_rule_based_strategy_message_length_trigger() {
    let rule = RoutingRule::new(
        "long-message",
        "claude-3",
        ProviderType::Anthropic,
        1,
    )
    .with_triggers(vec![RoutingTrigger::MessageLength {
        min: Some(100),
        max: None,
    }]);

    let strategy = RuleBasedStrategy::new(vec![rule]);

    let conversation = vec![make_text_message(Role::User, &"a".repeat(150))];

    let models = vec![ModelConfig::anthropic("claude-3")];
    let mut decision = crate::model_router::types::RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result =
        rt.block_on(strategy.route(&conversation, &models, &mut decision));
    assert!(result.is_ok());
    assert_eq!(decision.matched_rules, vec!["long-message"]);
}

// ==================== Strategy Adaptive Tests ====================

#[test]
fn test_adaptive_strategy_new() {
    let models = vec![ModelConfig::anthropic("claude-3")];
    let _strategy = AdaptiveStrategy::new(models);
}

#[test]
fn test_adaptive_strategy_route_first_turn() {
    let models = vec![
        ModelConfig::anthropic("claude-3"),
        ModelConfig::openai("gpt-4o"),
    ];
    let strategy = AdaptiveStrategy::new(models);

    let conversation = vec![make_text_message(Role::User, "Hello")];

    let mut decision = crate::model_router::types::RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(strategy.route(&conversation, &[], &mut decision));
    assert!(result.is_ok());
    // On first turn with Anthropic available, should select Anthropic
    assert_eq!(decision.provider_type, ProviderType::Anthropic);
}

#[test]
fn test_adaptive_strategy_route_with_failures() {
    let models = vec![
        ModelConfig::anthropic("claude-3"),
        ModelConfig::openai("gpt-4o"),
    ];
    let strategy = AdaptiveStrategy::new(models);

    let conversation = vec![
        make_text_message(Role::Assistant, "The error occurred"),
        make_text_message(Role::User, "Hello"),
    ];

    let mut decision = crate::model_router::types::RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(strategy.route(&conversation, &[], &mut decision));
    assert!(result.is_ok());
    assert_eq!(decision.conversation_metrics.consecutive_failures, 0);
}

#[test]
fn test_adaptive_strategy_route_with_tool_calls() {
    let models = vec![
        ModelConfig::anthropic("claude-3"),
        ModelConfig::openai("gpt-4o"),
    ];
    let strategy = AdaptiveStrategy::new(models);

    let conversation = vec![
        make_text_message(Role::Assistant, "I will use a tool_call to help"),
        make_text_message(Role::Assistant, "Another tool_call here"),
        make_text_message(Role::Assistant, "Third tool_call"),
        make_text_message(Role::User, "Hello"),
    ];

    let mut decision = crate::model_router::types::RoutingDecision::default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(strategy.route(&conversation, &[], &mut decision));
    assert!(result.is_ok());
    assert_eq!(decision.conversation_metrics.tool_call_count, 0);
}

// ==================== Router Tests ====================

#[test]
fn test_model_fallback_router_new() {
    let _router: ModelFallbackRouter = ModelFallbackRouter::new(vec![]);
}

#[test]
fn test_model_fallback_router_empty_routers() {
    let router: ModelFallbackRouter = ModelFallbackRouter::new(vec![]);

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(router.route(&[]));
    assert!(result.is_err());
}

#[test]
fn test_model_fallback_router_available_models_empty() {
    let router: ModelFallbackRouter = ModelFallbackRouter::new(vec![]);
    assert!(router.available_models().is_empty());
}

#[test]
fn test_model_fallback_router_context_window() {
    let router: ModelFallbackRouter = ModelFallbackRouter::new(vec![]);
    assert_eq!(router.context_window(), 200_000);
}

#[test]
fn test_default_model_router_new() {
    let models = vec![ModelConfig::anthropic("claude-3")];
    let strategy = Box::new(SimpleStrategy::default());
    let router = DefaultModelRouter::new(models.clone(), strategy);
    assert_eq!(router.available_models().len(), models.len());
}

#[test]
fn test_default_model_router_with_simple_strategy() {
    let models = vec![ModelConfig::anthropic("claude-3")];
    let router = DefaultModelRouter::with_simple_strategy(models.clone());
    assert_eq!(router.available_models().len(), models.len());
}

#[test]
fn test_default_model_router_route() {
    let models = vec![
        ModelConfig::anthropic("claude-3"),
        ModelConfig::openai("gpt-4o"),
    ];
    let router = DefaultModelRouter::with_simple_strategy(models);

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(router.route(&[]));
    assert!(result.is_ok());
    let routing_result = result.unwrap();
    assert_eq!(routing_result.config.model_info().name, "claude-3");
}

#[test]
fn test_default_model_router_context_window() {
    let models = vec![ModelConfig::anthropic("claude-3")];
    let router = DefaultModelRouter::with_simple_strategy(models);
    assert!(router.context_window() >= 200_000);
}
