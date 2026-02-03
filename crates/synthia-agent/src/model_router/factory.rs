use std::sync::Arc;

use synthia_provider::{
    AnthropicProvider,
    ModelProvider,
    OpenAICompatibleProvider,
};

use crate::{AgentError, model_router::types::ModelConfig};

#[derive(Debug, Clone)]
pub struct ProviderFactory;

impl ProviderFactory {
    pub fn new() -> Self {
        Self
    }

    pub fn create(
        &self,
        config: &ModelConfig,
    ) -> Result<Arc<dyn ModelProvider>, AgentError> {
        match config {
            ModelConfig::Anthropic(info) => {
                let provider =
                    apply_provider_config(AnthropicProvider::default(), info);
                Ok(Arc::new(provider))
            }
            ModelConfig::OpenAI(info) => {
                let provider = apply_provider_config(
                    OpenAICompatibleProvider::default(),
                    info,
                );
                Ok(Arc::new(provider))
            }
            ModelConfig::OpenAICompatible { info, base_url } => {
                let provider = apply_provider_config(
                    OpenAICompatibleProvider::default().with_base_url(base_url),
                    info,
                );
                Ok(Arc::new(provider))
            }
            ModelConfig::Custom {
                provider_type,
                info,
            } => self.create_custom(provider_type, info),
        }
    }

    fn create_custom(
        &self,
        provider_type: &str,
        info: &crate::model_router::types::ModelInfo,
    ) -> Result<Arc<dyn ModelProvider>, AgentError> {
        match provider_type {
            "openai-compatible" => {
                let base_url = info
                    .base_url
                    .as_deref()
                    .unwrap_or("https://api.openai.com/v1");
                let provider = apply_provider_config(
                    OpenAICompatibleProvider::default().with_base_url(base_url),
                    info,
                );
                Ok(Arc::new(provider))
            }
            _ => Err(AgentError::ConfigError(format!(
                "Unsupported provider type: {provider_type}"
            ))),
        }
    }
}

fn apply_provider_config<P: ProviderConfig>(
    mut provider: P,
    info: &crate::model_router::types::ModelInfo,
) -> P {
    if let Some(ref url) = info.base_url {
        provider = provider.with_base_url(url);
    }
    if let Some(ref key) = info.api_key {
        provider = provider.with_api_key(key);
    }
    provider
}

trait ProviderConfig: Sized {
    fn with_base_url(self, url: &str) -> Self;
    fn with_api_key(self, key: &str) -> Self;
}

impl ProviderConfig for AnthropicProvider {
    fn with_base_url(self, url: &str) -> Self {
        self.with_base_url(url)
    }

    fn with_api_key(self, key: &str) -> Self {
        self.with_api_key(key)
    }
}

impl ProviderConfig for OpenAICompatibleProvider {
    fn with_base_url(self, url: &str) -> Self {
        self.with_base_url(url)
    }

    fn with_api_key(self, key: &str) -> Self {
        self.with_api_key(key)
    }
}

impl Default for ProviderFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_router::types::{ModelCapabilities, ModelInfo};

    fn make_model_info(name: &str) -> ModelInfo {
        ModelInfo {
            name: name.to_string(),
            api_key: Some("test-key".to_string()),
            base_url: Some("https://api.example.com".to_string()),
            context_window: Some(200000),
            description: None,
            capabilities: None,
            temperature: Some(0.7),
            max_tokens: 4096,
        }
    }

    #[test]
    fn test_provider_factory_new() {
        let factory = ProviderFactory::new();
        // Just verify it can be created
        let _ = factory;
    }

    #[test]
    fn test_provider_factory_create_anthropic() {
        let factory = ProviderFactory::new();
        let config = ModelConfig::Anthropic(make_model_info("claude-3"));
        let result = factory.create(&config);
        assert!(result.is_ok());
        // Verify we got a provider back
        let _provider = result.unwrap();
    }

    #[test]
    fn test_provider_factory_create_openai() {
        let factory = ProviderFactory::new();
        let config = ModelConfig::OpenAI(make_model_info("gpt-4o"));
        let result = factory.create(&config);
        assert!(result.is_ok());
        let _provider = result.unwrap();
    }

    #[test]
    fn test_provider_factory_create_openai_compatible() {
        let factory = ProviderFactory::new();
        let config = ModelConfig::OpenAICompatible {
            info: make_model_info("custom-model"),
            base_url: "https://api.custom.com".to_string(),
        };
        let result = factory.create(&config);
        assert!(result.is_ok());
        let _provider = result.unwrap();
    }

    #[test]
    fn test_provider_factory_create_custom_openai_compatible() {
        let factory = ProviderFactory::new();
        let config = ModelConfig::Custom {
            provider_type: "openai-compatible".to_string(),
            info: make_model_info("custom-model"),
        };
        let result = factory.create(&config);
        assert!(result.is_ok());
        let _provider = result.unwrap();
    }

    #[test]
    fn test_provider_factory_create_custom_unsupported_type() {
        let factory = ProviderFactory::new();
        let config = ModelConfig::Custom {
            provider_type: "unsupported-provider".to_string(),
            info: make_model_info("test-model"),
        };
        let result = factory.create(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_provider_factory_create_custom_without_base_url() {
        let factory = ProviderFactory::new();
        let mut info = make_model_info("custom-model");
        info.base_url = None;
        let config = ModelConfig::Custom {
            provider_type: "openai-compatible".to_string(),
            info,
        };
        let result = factory.create(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_provider_factory_create_anthropic_without_api_key() {
        let factory = ProviderFactory::new();
        let mut info = make_model_info("claude-3");
        info.api_key = None;
        let config = ModelConfig::Anthropic(info);
        let result = factory.create(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_provider_factory_create_openai_without_base_url() {
        let factory = ProviderFactory::new();
        let mut info = make_model_info("gpt-4o");
        info.base_url = None;
        let config = ModelConfig::OpenAI(info);
        let result = factory.create(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_provider_factory_create_with_capabilities() {
        let factory = ProviderFactory::new();
        let mut info = make_model_info("claude-3");
        info.capabilities = Some(ModelCapabilities { vision: Some(true) });
        let config = ModelConfig::Anthropic(info);
        let result = factory.create(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_provider_factory_default() {
        let factory = ProviderFactory;
        let config = ModelConfig::anthropic("claude-3");
        let result = factory.create(&config);
        assert!(result.is_ok());
    }
}
