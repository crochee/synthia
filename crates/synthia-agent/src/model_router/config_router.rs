use async_trait::async_trait;
use rmcp::model::SamplingMessage;
use tracing::instrument;

use super::types::{ModelConfig, ModelRouter, RoutingResult};
use crate::{Result, model_router::ProviderFactory};

#[derive(Debug, Clone)]
pub struct FirstModelRouter {
    models: Vec<ModelConfig>,
    provider_factory: ProviderFactory,
}

impl FirstModelRouter {
    pub fn new(models: Vec<ModelConfig>) -> Self {
        Self {
            models,
            provider_factory: ProviderFactory::new(),
        }
    }
}

impl Default for FirstModelRouter {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

#[async_trait]
impl ModelRouter for FirstModelRouter {
    #[instrument(skip_all)]
    async fn route(
        &self,
        _conversation: &[SamplingMessage],
    ) -> Result<RoutingResult> {
        let model_config = self.models.first().cloned().ok_or_else(|| {
            crate::AgentError::ConfigError("No models configured".to_string())
        })?;

        tracing::info!("Using model: {:?}", model_config);
        let provider = self.provider_factory.create(&model_config)?;

        Ok(RoutingResult {
            provider,
            config: model_config,
            decision: super::types::RoutingDecision::default(),
        })
    }

    fn available_models(&self) -> &[ModelConfig] {
        &self.models
    }

    fn context_window(&self) -> usize {
        self.models
            .first()
            .and_then(|m| m.model_info().context_window)
            .unwrap_or(200_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_router::types::ModelInfo;

    fn _make_model_info(
        name: &str,
        context_window: Option<usize>,
    ) -> ModelConfig {
        let mut info = ModelInfo::with_name(name);
        info.context_window = context_window;
        ModelConfig::Anthropic(info)
    }

    #[test]
    fn test_first_model_router_new() {
        let models = vec![ModelConfig::anthropic("claude-3")];
        let router = FirstModelRouter::new(models);
        assert_eq!(router.available_models().len(), 1);
    }

    #[test]
    fn test_first_model_router_default() {
        let router = FirstModelRouter::default();
        assert!(router.available_models().is_empty());
    }

    #[test]
    fn test_first_model_router_route_returns_first_model() {
        let models = vec![
            ModelConfig::anthropic("claude-3"),
            ModelConfig::openai("gpt-4o"),
        ];
        let router = FirstModelRouter::new(models);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(router.route(&[]));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().config.model_info().name, "claude-3");
    }

    #[test]
    fn test_first_model_router_route_empty_models_returns_error() {
        let router = FirstModelRouter::new(vec![]);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(router.route(&[]));
        assert!(result.is_err());
    }

    #[test]
    fn test_first_model_router_available_models() {
        let models = vec![
            ModelConfig::anthropic("claude-3"),
            ModelConfig::openai("gpt-4o"),
        ];
        let router = FirstModelRouter::new(models);
        assert_eq!(router.available_models().len(), 2);
    }

    #[test]
    fn test_first_model_router_context_window_with_config() {
        let mut info = ModelInfo::with_name("claude-3");
        info.context_window = Some(180_000);
        let models = vec![ModelConfig::Anthropic(info)];
        let router = FirstModelRouter::new(models);

        assert_eq!(router.context_window(), 180_000);
    }

    #[test]
    fn test_first_model_router_context_window_without_config() {
        let models = vec![ModelConfig::anthropic("claude-3")];
        let router = FirstModelRouter::new(models);

        assert_eq!(router.context_window(), 200_000);
    }

    #[test]
    fn test_first_model_router_context_window_empty_models() {
        let router = FirstModelRouter::new(vec![]);
        assert_eq!(router.context_window(), 200_000);
    }

    #[test]
    fn test_first_model_router_route_ignores_conversation() {
        // FirstModelRouter ignores conversation and always returns first model
        let models = vec![
            ModelConfig::anthropic("claude-3"),
            ModelConfig::openai("gpt-4o"),
        ];
        let router = FirstModelRouter::new(models);

        let conversation = vec![]; // empty conversation

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(router.route(&conversation));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().config.model_info().name, "claude-3");
    }
}
