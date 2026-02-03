use async_trait::async_trait;
use rmcp::model::SamplingMessage;

use crate::{
    Result,
    model_router::types::{ModelConfig, RoutingDecision, RoutingStrategy},
};

pub struct SimpleStrategy {
    default_model_index: usize,
}

impl SimpleStrategy {
    pub fn new(default_index: usize) -> Self {
        Self {
            default_model_index: default_index,
        }
    }
}

impl Default for SimpleStrategy {
    fn default() -> Self {
        Self::new(0)
    }
}

#[async_trait]
impl RoutingStrategy for SimpleStrategy {
    async fn route(
        &self,
        _conversation: &[SamplingMessage],
        models: &[ModelConfig],
        decision: &mut RoutingDecision,
    ) -> Result<ModelConfig> {
        let model = models
            .get(self.default_model_index)
            .or_else(|| models.first())
            .cloned()
            .ok_or_else(|| {
                crate::AgentError::ConfigError(
                    "No models available".to_string(),
                )
            })?;

        decision.reasoning = "Selected default model".to_string();
        decision.selected_model = model.model_info().name.clone();
        decision.provider_type = model.provider_type();

        Ok(model)
    }

    fn name(&self) -> &'static str {
        "simple"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_strategy_new_sets_default_index() {
        let strategy = SimpleStrategy::new(0);
        assert_eq!(strategy.name(), "simple");
    }

    #[test]
    fn test_simple_strategy_new_with_nonzero_index() {
        let strategy = SimpleStrategy::new(2);
        assert_eq!(strategy.name(), "simple");
    }

    #[test]
    fn test_simple_strategy_default_is_index_zero() {
        let strategy = SimpleStrategy::default();
        assert_eq!(strategy.name(), "simple");
    }
}
