use std::{collections::HashMap, fmt};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskType {
    CodeGeneration,
    TextAnalysis,
    CreativeWriting,
    Reasoning,
    QuestionAnswering,
    ToolUse,
    Compaction,
}

impl fmt::Display for TaskType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskType::CodeGeneration => write!(f, "code_generation"),
            TaskType::TextAnalysis => write!(f, "text_analysis"),
            TaskType::CreativeWriting => write!(f, "creative_writing"),
            TaskType::Reasoning => write!(f, "reasoning"),
            TaskType::QuestionAnswering => write!(f, "question_answering"),
            TaskType::ToolUse => write!(f, "tool_use"),
            TaskType::Compaction => write!(f, "compaction"),
        }
    }
}

impl Serialize for TaskType {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for TaskType {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "code_generation" => Ok(TaskType::CodeGeneration),
            "text_analysis" => Ok(TaskType::TextAnalysis),
            "creative_writing" => Ok(TaskType::CreativeWriting),
            "reasoning" => Ok(TaskType::Reasoning),
            "question_answering" => Ok(TaskType::QuestionAnswering),
            "tool_use" => Ok(TaskType::ToolUse),
            "compaction" => Ok(TaskType::Compaction),
            _ => Err(serde::de::Error::unknown_variant(
                &s,
                &[
                    "code_generation",
                    "text_analysis",
                    "creative_writing",
                    "reasoning",
                    "question_answering",
                    "tool_use",
                    "compaction",
                ],
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComplexityLevel {
    Simple,
    Medium,
    Complex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LatencySensitivity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: String,
    pub end: String,
    pub days: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum RoutingCondition {
    Complexity(ComplexityLevel),
    ToolRequired(bool),
    StreamingRequired(bool),
    CostBudget(f64),
    LatencySensitivity(LatencySensitivity),
    TimeRange(TimeRange),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub condition: RoutingCondition,
    pub provider_name: String,
    pub model_name: String,
    pub priority: usize,
}

#[derive(Debug, Clone)]
pub struct RoutingContext {
    pub request: crate::CompletionRequest,
    pub cost_budget: Option<f64>,
    pub streaming_required: bool,
    pub latency_sensitivity: Option<LatencySensitivity>,
}

impl RoutingContext {
    pub fn new(request: crate::CompletionRequest) -> Self {
        Self {
            request,
            cost_budget: None,
            streaming_required: false,
            latency_sensitivity: None,
        }
    }

    pub fn with_cost_budget(mut self, budget: f64) -> Self {
        self.cost_budget = Some(budget);
        self
    }

    pub fn with_latency_sensitivity(
        mut self,
        sensitivity: LatencySensitivity,
    ) -> Self {
        self.latency_sensitivity = Some(sensitivity);
        self
    }

    pub fn with_streaming_required(mut self) -> Self {
        self.streaming_required = true;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteEntry {
    pub provider: String,
    pub model: String,
    pub fallback: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RoutingConfig {
    pub routes: HashMap<TaskType, RouteEntry>,
}

#[derive(Debug, Deserialize)]
pub struct TomlModelRouting {
    pub routes: HashMap<TaskType, RouteEntry>,
}

#[derive(Debug, Deserialize)]
pub struct TomlRoutingConfig {
    pub model_routing: TomlModelRouting,
}

impl RoutingConfig {
    pub fn from_toml(content: &str) -> Result<Self, String> {
        let parsed: TomlRoutingConfig = toml::from_str(content)
            .map_err(|e| format!("Failed to parse TOML: {}", e))?;
        Ok(Self {
            routes: parsed.model_routing.routes,
        })
    }

    pub fn get_route(&self, task_type: &TaskType) -> Option<&RouteEntry> {
        self.routes.get(task_type)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn test_complexity_levels() {
        assert_eq!(ComplexityLevel::Simple, ComplexityLevel::Simple);
        assert_ne!(ComplexityLevel::Simple, ComplexityLevel::Complex);
    }

    #[test]
    fn test_routing_condition_complexity() {
        let cond = RoutingCondition::Complexity(ComplexityLevel::Simple);
        if let RoutingCondition::Complexity(level) = cond {
            assert_eq!(level, ComplexityLevel::Simple);
        }
    }

    #[test]
    fn test_routing_condition_tool_required() {
        let cond = RoutingCondition::ToolRequired(true);
        if let RoutingCondition::ToolRequired(required) = cond {
            assert!(required);
        }
    }

    #[test]
    fn test_routing_rule() {
        let rule = RoutingRule {
            condition: RoutingCondition::Complexity(ComplexityLevel::Simple),
            provider_name: "openai".to_string(),
            model_name: "gpt-4o-mini".to_string(),
            priority: 1,
        };
        assert_eq!(rule.priority, 1);
    }

    #[test]
    fn test_routing_context() {
        let request = crate::CompletionRequest {
            model: "test".to_string(),
            messages: Arc::new(vec![]),
            tools: Arc::new(vec![]),
            tool_choice: crate::ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
        };
        let ctx = RoutingContext::new(request);
        assert_eq!(ctx.request.model, "test");
        assert!(ctx.cost_budget.is_none());
    }

    #[test]
    fn test_route_entry_without_fallback() {
        let entry = RouteEntry {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            fallback: None,
        };
        assert_eq!(entry.provider, "openai");
        assert!(entry.fallback.is_none());
    }

    #[test]
    fn test_route_entry_with_fallback() {
        let entry = RouteEntry {
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4".to_string(),
            fallback: Some("openai".to_string()),
        };
        assert_eq!(entry.fallback, Some("openai".to_string()));
    }

    #[test]
    fn test_routing_config_from_toml() {
        let toml_str = r#"
[model_routing.routes.code_generation]
provider = "anthropic"
model = "claude-sonnet-4"
fallback = "openai"

[model_routing.routes.tool_use]
provider = "openai"
model = "gpt-4o"
"#;
        let config: super::TomlRoutingConfig =
            toml::from_str(toml_str).unwrap();
        assert_eq!(config.model_routing.routes.len(), 2);

        let code_gen = &config.model_routing.routes[&TaskType::CodeGeneration];
        assert_eq!(code_gen.provider, "anthropic");
        assert_eq!(code_gen.model, "claude-sonnet-4");
        assert_eq!(code_gen.fallback, Some("openai".to_string()));

        let tool_use = &config.model_routing.routes[&TaskType::ToolUse];
        assert_eq!(tool_use.provider, "openai");
        assert!(tool_use.fallback.is_none());
    }

    #[test]
    fn test_routing_config_build() {
        let mut routes = HashMap::new();
        routes.insert(
            TaskType::Reasoning,
            RouteEntry {
                provider: "openai".to_string(),
                model: "o3".to_string(),
                fallback: Some("anthropic".to_string()),
            },
        );
        let config = RoutingConfig { routes };
        assert_eq!(config.routes.len(), 1);
        let entry = config.routes.get(&TaskType::Reasoning).unwrap();
        assert_eq!(entry.model, "o3");
    }
}
