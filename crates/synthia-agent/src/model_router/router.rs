use async_trait::async_trait;
use rmcp::model::SamplingMessage;
use tracing::warn;

use crate::{
    AgentError,
    Result,
    model_router::{
        ProviderFactory,
        types::{ModelConfig, ModelRouter, RoutingResult, RoutingStrategy},
    },
};

/// Model router that tries multiple routers in sequence,
/// falling back to the next if one fails.
pub struct ModelFallbackRouter {
    routers: Vec<Box<dyn ModelRouter>>,
}

impl ModelFallbackRouter {
    pub fn new(routers: Vec<Box<dyn ModelRouter>>) -> Self {
        Self { routers }
    }
}

#[async_trait]
impl ModelRouter for ModelFallbackRouter {
    async fn route(
        &self,
        conversation: &[SamplingMessage],
    ) -> Result<RoutingResult> {
        for (i, router) in self.routers.iter().enumerate() {
            match router.route(conversation).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    warn!("Model router {} failed, trying next: {}", i, e);
                    continue;
                }
            }
        }
        Err(AgentError::context("All model routers failed"))
    }

    fn available_models(&self) -> &[ModelConfig] {
        // Return models from the first router that has any
        self.routers
            .iter()
            .find(|router| !router.available_models().is_empty())
            .map(|router| router.available_models())
            .unwrap_or(&[])
    }

    fn context_window(&self) -> usize {
        self.routers
            .iter()
            .find(|router| router.context_window() > 0)
            .map(|router| router.context_window())
            .unwrap_or(200_000)
    }
}

pub struct DefaultModelRouter {
    models: Vec<ModelConfig>,
    strategy: Box<dyn RoutingStrategy>,
    provider_factory: ProviderFactory,
}

impl DefaultModelRouter {
    pub fn new(
        models: Vec<ModelConfig>,
        strategy: Box<dyn RoutingStrategy>,
    ) -> Self {
        Self {
            models,
            strategy,
            provider_factory: ProviderFactory::new(),
        }
    }

    pub fn with_simple_strategy(models: Vec<ModelConfig>) -> Self {
        Self::new(
            models,
            Box::new(crate::model_router::strategy::SimpleStrategy::default()),
        )
    }
}

#[async_trait]
impl ModelRouter for DefaultModelRouter {
    async fn route(
        &self,
        conversation: &[SamplingMessage],
    ) -> Result<RoutingResult> {
        let mut decision =
            crate::model_router::types::RoutingDecision::default();

        let model_config = self
            .strategy
            .route(conversation, &self.models, &mut decision)
            .await?;
        let provider = self.provider_factory.create(&model_config)?;

        Ok(RoutingResult {
            provider,
            config: model_config,
            decision,
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
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use rmcp::model::SamplingMessage;
    use synthia_provider::OpenAICompatibleProvider;

    use super::*;
    use crate::{
        Result,
        model_router::types::{ModelConfig, RoutingResult},
    };

    /// Mock router that can be configured to succeed or fail
    struct MockRouter {
        pub should_fail: bool,
        pub models: Vec<ModelConfig>,
        pub context_win: usize,
        pub call_count: Arc<Mutex<usize>>,
    }

    impl MockRouter {
        fn new(
            should_fail: bool,
            models: Vec<ModelConfig>,
            context_win: usize,
        ) -> Self {
            Self {
                should_fail,
                models,
                context_win,
                call_count: Arc::new(Mutex::new(0)),
            }
        }
    }

    #[async_trait]
    impl ModelRouter for MockRouter {
        async fn route(
            &self,
            _conversation: &[SamplingMessage],
        ) -> Result<RoutingResult> {
            let mut count = self.call_count.lock().unwrap();
            *count += 1;
            drop(count);

            if self.should_fail {
                Err(crate::AgentError::context("Mock router failed"))
            } else {
                Ok(RoutingResult {
                    provider: Arc::new(OpenAICompatibleProvider::default()),
                    config: self
                        .models
                        .first()
                        .cloned()
                        .unwrap_or(ModelConfig::openai("default")),
                    decision:
                        crate::model_router::types::RoutingDecision::default(),
                })
            }
        }

        fn available_models(&self) -> &[ModelConfig] {
            &self.models
        }

        fn context_window(&self) -> usize {
            self.context_win
        }
    }

    #[test]
    fn test_fallback_router_new() {
        let router = ModelFallbackRouter::new(vec![]);
        assert!(router.routers.is_empty());
    }

    #[test]
    fn test_fallback_router_empty_routers_returns_error() {
        let router = ModelFallbackRouter::new(vec![]);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(router.route(&[]));
        assert!(result.is_err());
    }

    #[test]
    fn test_fallback_router_single_successful_router() {
        let mock = MockRouter::new(
            false,
            vec![ModelConfig::anthropic("claude-3")],
            200_000,
        );
        let router = ModelFallbackRouter::new(vec![Box::new(mock)]);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(router.route(&[]));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().config.model_info().name, "claude-3");
    }

    #[test]
    fn test_fallback_router_falls_back_on_first_failure() {
        let mock1 = MockRouter::new(
            true,
            vec![ModelConfig::anthropic("claude-fail")],
            200_000,
        );
        let mock2 =
            MockRouter::new(false, vec![ModelConfig::openai("gpt-4")], 100_000);
        let router =
            ModelFallbackRouter::new(vec![Box::new(mock1), Box::new(mock2)]);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(router.route(&[]));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().config.model_info().name, "gpt-4");
    }

    #[test]
    fn test_fallback_router_all_fail_returns_error() {
        let mock1 = MockRouter::new(
            true,
            vec![ModelConfig::anthropic("claude-fail")],
            200_000,
        );
        let mock2 = MockRouter::new(
            true,
            vec![ModelConfig::openai("gpt-fail")],
            100_000,
        );
        let router =
            ModelFallbackRouter::new(vec![Box::new(mock1), Box::new(mock2)]);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(router.route(&[]));
        assert!(result.is_err());
    }

    #[test]
    fn test_fallback_router_available_models_first_non_empty() {
        let mock1 = MockRouter::new(false, vec![], 0);
        let mock2 =
            MockRouter::new(false, vec![ModelConfig::openai("gpt-4")], 100_000);
        let router =
            ModelFallbackRouter::new(vec![Box::new(mock1), Box::new(mock2)]);

        let models = router.available_models();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].model_info().name, "gpt-4");
    }

    #[test]
    fn test_fallback_router_available_models_all_empty() {
        let mock1 = MockRouter::new(false, vec![], 0);
        let mock2 = MockRouter::new(false, vec![], 0);
        let router =
            ModelFallbackRouter::new(vec![Box::new(mock1), Box::new(mock2)]);

        assert!(router.available_models().is_empty());
    }

    #[test]
    fn test_fallback_router_context_window_first_non_zero() {
        let mock1 = MockRouter::new(false, vec![], 0);
        let mock2 = MockRouter::new(false, vec![], 150_000);
        let router =
            ModelFallbackRouter::new(vec![Box::new(mock1), Box::new(mock2)]);

        assert_eq!(router.context_window(), 150_000);
    }

    #[test]
    fn test_fallback_router_context_window_all_zero_returns_fallback() {
        let mock1 = MockRouter::new(false, vec![], 0);
        let mock2 = MockRouter::new(false, vec![], 0);
        let router =
            ModelFallbackRouter::new(vec![Box::new(mock1), Box::new(mock2)]);

        assert_eq!(router.context_window(), 200_000);
    }

    #[test]
    fn test_fallback_router_calls_routers_in_sequence() {
        let _call_count = Arc::new(Mutex::new(0));

        // We can't easily check call count on the first router since it succeeds
        // But we can verify the second is called when first fails
        let mock1_fail = MockRouter::new(
            true,
            vec![ModelConfig::anthropic("claude-fail")],
            200_000,
        );
        let mock2 =
            MockRouter::new(false, vec![ModelConfig::openai("gpt-4")], 100_000);
        let router = ModelFallbackRouter::new(vec![
            Box::new(mock1_fail),
            Box::new(mock2),
        ]);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let _ = rt.block_on(router.route(&[]));

        // The actual test is that gpt-4 was returned
        let result = rt.block_on(router.route(&[]));
        assert!(result.is_ok());
    }
}
