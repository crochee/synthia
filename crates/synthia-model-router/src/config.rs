//! Model router configuration

use serde::{Deserialize, Serialize};

/// Routing condition for model selection
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum RoutingCondition {
    /// Route based on task complexity
    Complexity(ComplexityLevel),
    /// Route based on whether tools are required
    ToolRequired(bool),
    /// Route based on streaming requirement
    StreamingRequired(bool),
    /// Route based on cost budget (max cost per request)
    CostBudget(f64),
}

/// Task complexity level
#[derive(
    Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum ComplexityLevel {
    /// Simple queries, formatting
    Simple,
    /// Code generation, analysis
    Medium,
    /// Complex reasoning, multi-step tasks
    Complex,
}

/// Routing rule configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoutingRule {
    pub condition: RoutingCondition,
    pub provider_name: String,
    pub model_name: String,
    pub priority: usize,
}

/// Fallback chain configuration
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct FallbackChainConfig {
    #[serde(flatten)]
    pub chains: std::collections::HashMap<String, Vec<String>>,
}

/// Complete model router configuration
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ModelRouterConfig {
    pub routing_rules: Vec<RoutingRule>,
    pub fallback_chain: FallbackChainConfig,
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
}

impl ModelRouterConfig {
    pub fn from_toml(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }
}
