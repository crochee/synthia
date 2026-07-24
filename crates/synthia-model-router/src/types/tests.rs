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
