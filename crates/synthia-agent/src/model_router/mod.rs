//! Model router module
//!
//! This module provides functionality to route tasks to appropriate models
//! based on task type and conversation content.
//!
//! # Architecture
//!
//! The model routing system consists of several components:
//! - [`ModelRouter`]: Core trait for routing
//! - [`RoutingStrategy`]: Pluggable strategies for selecting models
//! - [`ProviderFactory`]: Factory for creating model providers
//! - [`ConversationAnalyzer`]: Analyzes conversation for routing decisions
//!
//! # Example
//!
//! ```rust,ignore
//! use synthia_agent::model_router::{
//!     DefaultModelRouter, ModelConfig, RoutingStrategy,
//!     strategy::{SimpleStrategy, RuleBasedStrategy, RoutingRule, RoutingTrigger, KeywordMatch}
//! };
//! use rmcp::model::SamplingMessage;
//!
//! async fn route_model() {
//!     let models = vec![
//!         ModelConfig::anthropic("claude-3-opus"),
//!         ModelConfig::openai("gpt-4o"),
//!     ];
//!
//!     let router = DefaultModelRouter::with_simple_strategy(models);
//!     let conversation = vec![];
//!
//!     let result = router.route(&conversation).await.unwrap();
//!     println!("Selected: {:?}", result.decision.selected_model);
//! }
//! ```

pub mod analyzer;
pub mod cache;
pub mod config_router;
pub mod factory;
pub mod router;
pub mod strategy;
pub mod types;

#[cfg(test)]
mod tests;

pub use analyzer::ConversationAnalyzer;
pub use cache::{ModelEntry, ModelList, ModelsCacheManager};
pub use config_router::FirstModelRouter;
pub use factory::ProviderFactory;
pub use router::DefaultModelRouter;
pub use strategy::{
    AdaptiveStrategy,
    RuleBasedStrategy,
    SimpleStrategy,
    rule_based::RoutingRule,
};
pub use types::{
    Comparison,
    ComplexityLevel,
    ConversationMetrics,
    KeywordMatch,
    ModelCapabilities,
    ModelConfig,
    ModelInfo,
    ModelRouter,
    ProviderType,
    RoutingDecision,
    RoutingResult,
    RoutingStrategy,
    RoutingTrigger,
};
