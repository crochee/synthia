use synthia_core::{Error, registry::Registry};

use super::*;
use crate::traits::ModelProvider;

#[derive(Debug)]
struct TestProvider;

#[async_trait::async_trait]
impl ModelProvider for TestProvider {
    fn name(&self) -> &str {
        "test_provider"
    }

    async fn initialize(
        &mut self,
        _config: crate::types::ProviderConfig,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn model_config(&self) -> crate::types::ModelConfig {
        crate::types::ModelConfig {
            name: "test".to_string(),
            provider: "test".to_string(),
            context_window: 128_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        }
    }

    async fn complete(
        &self,
        _request: crate::types::CompletionRequest,
    ) -> Result<crate::types::CompletionResponse, Error> {
        Err(Error::Provider(
            "TestProvider does not support complete".to_string(),
        ))
    }

    async fn embed(&self, _texts: Vec<String>) -> Result<Vec<Vec<f64>>, Error> {
        Err(Error::Provider(
            "TestProvider does not support embed".to_string(),
        ))
    }
}

#[test]
fn test_provider_registry_register() {
    let registry = ProviderRegistry::new();
    registry.register_provider(Box::new(TestProvider));
    assert!(registry.contains("test_provider"));
    assert!(!registry.contains("nonexistent"));
}

#[tokio::test]
async fn test_provider_registry_list_names() {
    let registry = ProviderRegistry::new();
    registry.register_provider(Box::new(TestProvider));
    let items = registry.list(None).await.unwrap();
    let names: Vec<String> = items.iter().map(|p| p.name.clone()).collect();
    assert_eq!(names.len(), 1);
    assert_eq!(names[0], "test_provider");
}

#[tokio::test]
async fn test_registry_trait_get() {
    let registry = ProviderRegistry::new();
    registry.register_provider(Box::new(TestProvider));

    let item = registry.get("test_provider").await.unwrap();
    assert_eq!(item.unwrap().name, "test_provider");

    let not_found = registry.get("nonexistent").await.unwrap();
    assert!(not_found.is_none());
}

#[tokio::test]
async fn test_registry_trait_list() {
    let registry = ProviderRegistry::new();
    registry.register_provider(Box::new(TestProvider));

    let items = registry.list(None).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "test_provider");
}

#[tokio::test]
async fn test_registry_trait_unregister() {
    let registry = ProviderRegistry::new();
    registry.register_provider(Box::new(TestProvider));

    assert!(registry.contains("test_provider"));
    registry.unregister("test_provider").await.unwrap();
    assert!(!registry.contains("test_provider"));
}

#[tokio::test]
async fn test_registry_trait_unregister_not_found() {
    let registry = ProviderRegistry::new();
    let result = registry.unregister("nonexistent").await;
    assert!(result.is_err());
}
