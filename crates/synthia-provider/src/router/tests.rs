//! Tests for the model router.
//!
//! These cover rule evaluation, complexity analysis, fallback
//! chains, routing-config reload, and the various selection methods.

use std::{collections::HashMap, sync::Arc};

use synthia_core::Error;

use super::*;
use crate::ToolChoice;

fn make_request(messages: usize, tools: usize) -> crate::CompletionRequest {
    let msgs: Vec<_> = (0..messages)
        .map(|i| crate::Message::user(format!("msg {}", i)))
        .collect();
    let tool_list: Vec<crate::ToolDefinition> = (0..tools)
        .map(|i| crate::ToolDefinition {
            name: format!("tool_{}", i),
            description: format!("Test tool {}", i),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
            cache_control: None,
        })
        .collect();
    crate::CompletionRequest {
        model: "test".to_string(),
        messages: Arc::new(msgs),
        tools: Arc::new(tool_list),
        tool_choice: ToolChoice::Auto,
        temperature: None,
        max_tokens: None,
        stop_sequences: vec![],
        extra_body: None,
        cache_policy: None,
    }
}

fn make_config(supports_tools: bool) -> crate::ModelConfig {
    crate::ModelConfig {
        name: "test".to_string(),
        provider: "openai".to_string(),
        context_window: 128000,
        max_output_tokens: 4096,
        supports_tools,
        supports_streaming: true,
        supports_reasoning: false,
    }
}

fn make_config_with_provider(
    provider: &str,
    supports_tools: bool,
    context_window: usize,
) -> crate::ModelConfig {
    crate::ModelConfig {
        name: "test".to_string(),
        provider: provider.to_string(),
        context_window,
        max_output_tokens: 4096,
        supports_tools,
        supports_streaming: true,
        supports_reasoning: false,
    }
}

#[test]
fn test_rule_evaluator_complexity_match() {
    let rules = vec![RoutingRule {
        condition: RoutingCondition::Complexity(ComplexityLevel::Simple),
        provider_name: "openai".to_string(),
        model_name: "gpt-4o-mini".to_string(),
        priority: 1,
    }];
    let request = make_request(3, 0);
    let context = RoutingContext::new(request);
    let result = RuleEvaluator::evaluate(&rules, &context);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().model_name, "gpt-4o-mini");
}

#[test]
fn test_rule_evaluator_no_match() {
    let rules = vec![RoutingRule {
        condition: RoutingCondition::Complexity(ComplexityLevel::Complex),
        provider_name: "openai".to_string(),
        model_name: "gpt-4".to_string(),
        priority: 1,
    }];
    let request = make_request(3, 0);
    let context = RoutingContext::new(request);
    let result = RuleEvaluator::evaluate(&rules, &context);
    assert!(result.is_err());
}

#[test]
fn test_rule_evaluator_priority() {
    let rules = vec![
        RoutingRule {
            condition: RoutingCondition::Complexity(ComplexityLevel::Simple),
            provider_name: "openai".to_string(),
            model_name: "low-priority".to_string(),
            priority: 1,
        },
        RoutingRule {
            condition: RoutingCondition::Complexity(ComplexityLevel::Simple),
            provider_name: "anthropic".to_string(),
            model_name: "high-priority".to_string(),
            priority: 10,
        },
    ];
    let request = make_request(3, 0);
    let context = RoutingContext::new(request);
    let result = RuleEvaluator::evaluate(&rules, &context);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().model_name, "high-priority");
}

#[test]
fn test_tool_capable_model_selection() {
    let mut router = ModelRouter::new();
    router.register_provider("gpt-4".to_string(), make_config(true));
    router.register_provider("llama".to_string(), make_config(false));

    let request = make_request(5, 3);
    let context = RoutingContext::new(request);
    let result = router.select_tool_capable_model(&context);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "gpt-4");
}

#[test]
fn test_no_tool_capable_model() {
    let mut router = ModelRouter::new();
    router.register_provider("llama".to_string(), make_config(false));

    let request = make_request(5, 3);
    let context = RoutingContext::new(request);
    let result = router.select_tool_capable_model(&context);
    assert!(matches!(result, Err(Error::Router(_))));
}

#[test]
fn test_fallback_chain() {
    let mut router = ModelRouter::new();
    router.register_provider("backup".to_string(), make_config(true));
    router
        .set_fallback_chain("primary".to_string(), vec!["backup".to_string()]);
    let result = router.fallback_to_backup("primary");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "backup");
}

#[test]
fn test_select_model_with_fallback() {
    let mut router = ModelRouter::new();
    router.register_provider("gpt-4o-mini".to_string(), make_config(true));
    router.register_provider("gpt-4o".to_string(), make_config(true));
    router.add_rule(RoutingRule {
        condition: RoutingCondition::Complexity(ComplexityLevel::Simple),
        provider_name: "openai".to_string(),
        model_name: "gpt-4o-mini".to_string(),
        priority: 1,
    });
    router.set_fallback_chain(
        "gpt-4o-mini".to_string(),
        vec!["gpt-4o".to_string()],
    );

    let request = make_request(3, 0);
    let context = RoutingContext::new(request);
    let result = router.select_model(&context);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "gpt-4o-mini");
}

#[test]
fn test_select_within_budget() {
    let mut router = ModelRouter::new();
    router.register_provider("gpt-4o-mini".to_string(), make_config(true));
    router.register_provider("gpt-4".to_string(), make_config(true));
    router.set_model_cost("gpt-4o-mini".to_string(), 0.001);
    router.set_model_cost("gpt-4".to_string(), 0.01);

    let request = make_request(5, 0);
    let context = RoutingContext::new(request).with_cost_budget(0.005);
    let result = router.select_within_budget(&context);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "gpt-4o-mini");
}

#[test]
fn test_complexity_analysis_simple() {
    let router = ModelRouter::new();
    let request = make_request(3, 0);
    assert_eq!(router.analyze_complexity(&request), ComplexityLevel::Simple);
}

#[test]
fn test_complexity_analysis_medium() {
    let router = ModelRouter::new();
    let request = make_request(10, 2);
    assert_eq!(router.analyze_complexity(&request), ComplexityLevel::Medium);
}

#[test]
fn test_complexity_analysis_complex() {
    let router = ModelRouter::new();
    let request = make_request(25, 6);
    assert_eq!(
        router.analyze_complexity(&request),
        ComplexityLevel::Complex
    );
}

#[test]
fn test_load_routing_config_from_toml() {
    let mut router = ModelRouter::new();
    let toml_str = r#"
[model_routing.routes.code_generation]
provider = "anthropic"
model = "claude-sonnet-4"

[model_routing.routes.tool_use]
provider = "openai"
model = "gpt-4o"
fallback = "anthropic"
"#;
    let result = router.load_routing_config(toml_str);
    assert!(result.is_ok());

    let code_gen = router.routing_config.get_route(&TaskType::CodeGeneration);
    assert!(code_gen.is_some());
    let cg = code_gen.unwrap();
    assert_eq!(cg.provider, "anthropic");
    assert_eq!(cg.model, "claude-sonnet-4");
    assert!(cg.fallback.is_none());

    let tool_use = router.routing_config.get_route(&TaskType::ToolUse);
    assert!(tool_use.is_some());
    let tu = tool_use.unwrap();
    assert_eq!(tu.provider, "openai");
    assert_eq!(tu.model, "gpt-4o");
    assert_eq!(tu.fallback, Some("anthropic".to_string()));
}

#[test]
fn test_select_with_fallback_primary_succeeds() {
    let mut router = ModelRouter::new();
    router.register_provider(
        "gpt-4o".to_string(),
        make_config_with_provider("openai", true, 128000),
    );
    router.register_provider(
        "claude-sonnet-4".to_string(),
        make_config_with_provider("anthropic", true, 200000),
    );

    let toml_str = r#"
[model_routing.routes.code_generation]
provider = "openai"
model = "gpt-4o"
"#;
    router.load_routing_config(toml_str).unwrap();

    let result =
        router.select_with_fallback(TaskType::CodeGeneration, false, None);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "gpt-4o");
}

#[test]
fn test_select_with_fallback_no_route() {
    let router = ModelRouter::new();
    let result =
        router.select_with_fallback(TaskType::CodeGeneration, false, None);
    assert!(matches!(result, Err(Error::Router(_))));
}

#[test]
fn test_select_with_fallback_provider_not_registered() {
    let mut router = ModelRouter::new();
    let toml_str = r#"
[model_routing.routes.code_generation]
provider = "openai"
model = "unknown-model"
"#;
    router.load_routing_config(toml_str).unwrap();

    let result =
        router.select_with_fallback(TaskType::CodeGeneration, false, None);
    assert!(result.is_err());
}

#[test]
fn test_select_with_fallback_uses_fallback_on_primary_failure() {
    let mut router = ModelRouter::new();
    router.register_provider(
        "claude-sonnet-4".to_string(),
        make_config_with_provider("anthropic", true, 200000),
    );

    let toml_str = r#"
[model_routing.routes.code_generation]
provider = "openai"
model = "gpt-4o"
fallback = "anthropic"
"#;
    router.load_routing_config(toml_str).unwrap();

    let result =
        router.select_with_fallback(TaskType::CodeGeneration, false, None);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "claude-sonnet-4");
}

#[test]
fn test_select_with_fallback_tool_requirement() {
    let mut router = ModelRouter::new();
    router.register_provider(
        "gpt-4o-mini".to_string(),
        make_config_with_provider("openai", true, 128000),
    );

    let toml_str = r#"
[model_routing.routes.tool_use]
provider = "openai"
model = "gpt-4o-mini"
"#;
    router.load_routing_config(toml_str).unwrap();

    let result = router.select_with_fallback(TaskType::ToolUse, true, None);
    assert!(result.is_ok());

    let result = router.select_with_fallback(TaskType::ToolUse, false, None);
    assert!(result.is_ok());
}

#[test]
fn test_select_with_fallback_tool_requirement_no_tool_support() {
    let mut router = ModelRouter::new();
    router.register_provider(
        "llama-3".to_string(),
        make_config_with_provider("meta", false, 128000),
    );

    let toml_str = r#"
[model_routing.routes.code_generation]
provider = "meta"
model = "llama-3"
"#;
    router.load_routing_config(toml_str).unwrap();

    let result =
        router.select_with_fallback(TaskType::CodeGeneration, true, None);
    assert!(result.is_err());
}

#[test]
fn test_select_with_fallback_context_window_check() {
    let mut router = ModelRouter::new();
    router.register_provider(
        "small-model".to_string(),
        make_config_with_provider("openai", true, 8000),
    );

    let toml_str = r#"
[model_routing.routes.code_generation]
provider = "openai"
model = "small-model"
"#;
    router.load_routing_config(toml_str).unwrap();

    let result = router.select_with_fallback(
        TaskType::CodeGeneration,
        false,
        Some(100000),
    );
    assert!(matches!(result, Err(Error::Router(_))));

    let result = router.select_with_fallback(
        TaskType::CodeGeneration,
        false,
        Some(4000),
    );
    assert!(result.is_ok());
}

#[test]
fn test_select_with_fallback_chain_exhausted() {
    let mut router = ModelRouter::new();
    let toml_str = r#"
[model_routing.routes.code_generation]
provider = "openai"
model = "gpt-4o"
fallback = "anthropic"
"#;
    router.load_routing_config(toml_str).unwrap();

    let result =
        router.select_with_fallback(TaskType::CodeGeneration, false, None);
    assert!(result.is_err());
}

#[test]
fn test_reload_config() {
    let mut router = ModelRouter::new();
    router.register_provider(
        "gpt-4o".to_string(),
        make_config_with_provider("openai", true, 128000),
    );
    router.register_provider(
        "claude-sonnet-4".to_string(),
        make_config_with_provider("anthropic", true, 200000),
    );

    let toml_str = r#"
[model_routing.routes.code_generation]
provider = "openai"
model = "gpt-4o"
"#;
    router.load_routing_config(toml_str).unwrap();

    let result =
        router.select_with_fallback(TaskType::CodeGeneration, false, None);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "gpt-4o");

    let new_routes = {
        let mut routes = HashMap::new();
        routes.insert(
            TaskType::CodeGeneration,
            RouteEntry {
                provider: "anthropic".to_string(),
                model: "claude-sonnet-4".to_string(),
                fallback: None,
            },
        );
        RoutingConfig { routes }
    };
    router.reload_config(new_routes);

    let result =
        router.select_with_fallback(TaskType::CodeGeneration, false, None);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "claude-sonnet-4");
}

#[test]
fn test_reload_config_clears_old_routes() {
    let mut router = ModelRouter::new();
    router.register_provider(
        "gpt-4o".to_string(),
        make_config_with_provider("openai", true, 128000),
    );

    let toml_str = r#"
[model_routing.routes.code_generation]
provider = "openai"
model = "gpt-4o"
"#;
    router.load_routing_config(toml_str).unwrap();

    router.reload_config(RoutingConfig::default());

    let result =
        router.select_with_fallback(TaskType::CodeGeneration, false, None);
    assert!(matches!(result, Err(Error::Router(_))));
}

#[test]
fn test_reload_config_adds_new_routes() {
    let mut router = ModelRouter::new();
    router.register_provider(
        "gpt-4o".to_string(),
        make_config_with_provider("openai", true, 128000),
    );
    router.register_provider(
        "o3".to_string(),
        make_config_with_provider("openai", true, 200000),
    );

    assert!(
        router
            .select_with_fallback(TaskType::Reasoning, false, None)
            .is_err()
    );

    let mut routes = HashMap::new();
    routes.insert(
        TaskType::Reasoning,
        RouteEntry {
            provider: "openai".to_string(),
            model: "o3".to_string(),
            fallback: Some("gpt-4o".to_string()),
        },
    );
    router.reload_config(RoutingConfig { routes });

    let result = router.select_with_fallback(TaskType::Reasoning, false, None);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "o3");
}
