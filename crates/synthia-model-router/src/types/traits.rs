use async_trait::async_trait;
use synthia_provider::Message;

use super::{
    core::{Result, RoutingDecision, RoutingResult},
    model::ModelConfig,
};

#[async_trait]
pub trait RoutingStrategy: Send + Sync {
    async fn route(
        &self,
        conversation: &[Message],
        available_models: &[ModelConfig],
        decision: &mut RoutingDecision,
    ) -> Result<ModelConfig>;

    fn name(&self) -> &'static str;
}

#[async_trait]
pub trait ModelRouter: Send + Sync {
    async fn route(&self, conversation: &[Message]) -> Result<RoutingResult>;

    fn available_models(&self) -> &[ModelConfig];

    /// Returns the context window size for the currently selected model.
    /// Returns a fallback value if the context window is not known.
    fn context_window(&self) -> usize;
}
