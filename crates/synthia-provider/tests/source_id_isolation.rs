//! TDD: source_id isolation tests for `ProviderRegistry` v2.
//!
//! RED phase: written first; v2 module does not exist yet, so these tests
//! FAIL TO COMPILE until `crates/synthia-provider/src/registry/v2.rs`
//! and the re-exports in `registry/mod.rs` are added.

use std::sync::Arc;

use async_trait::async_trait;
use synthia_provider::{
    CompletionRequest,
    CompletionResponse,
    ModelConfig,
    ModelProvider,
    ProviderConfig,
    registry::v2::{ProviderRegistry, RegistryError, SourceId},
};

struct MockProvider;

#[async_trait]
impl ModelProvider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }

    fn model_config(&self) -> ModelConfig {
        ModelConfig {
            name: "mock".to_string(),
            provider: "mock".to_string(),
            context_window: 128_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        }
    }

    async fn initialize(
        &mut self,
        _config: ProviderConfig,
    ) -> Result<(), synthia_core::Error> {
        Ok(())
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, synthia_core::Error> {
        unimplemented!("MockProvider::complete is not needed by these tests")
    }

    async fn embed(
        &self,
        _texts: Vec<String>,
    ) -> Result<Vec<Vec<f64>>, synthia_core::Error> {
        unimplemented!("MockProvider::embed is not needed by these tests")
    }
}

#[tokio::test]
async fn two_sources_with_same_name_isolate() {
    let registry = ProviderRegistry::new();
    let p1: Arc<dyn ModelProvider> = Arc::new(MockProvider);
    let p2: Arc<dyn ModelProvider> = Arc::new(MockProvider);

    // First registration with source ext-a must succeed.
    registry
        .register("gpt-4", p1, SourceId("ext-a".to_string()))
        .await
        .expect("first registration with source ext-a should succeed");

    // Different source for the same name is allowed (isolation guarantee);
    // the new entry replaces the previous one (last-wins).
    registry
        .register("gpt-4", p2, SourceId("ext-b".to_string()))
        .await
        .expect("re-registration under a different source_id must succeed");

    // After both registrations the registry holds exactly one entry
    // for "gpt-4" (the most recent insertion) and it resolves to a value.
    let got = registry
        .get("gpt-4")
        .await
        .expect("get('gpt-4') must yield the surviving entry");
    assert_eq!(got.name(), "mock");
}

#[tokio::test]
async fn same_source_re_registration_rejects() {
    let registry = ProviderRegistry::new();
    let p: Arc<dyn ModelProvider> = Arc::new(MockProvider);

    registry
        .register("gpt-4", p.clone(), SourceId("ext-a".to_string()))
        .await
        .expect("first registration must succeed");

    let result = registry
        .register("gpt-4", p, SourceId("ext-a".to_string()))
        .await;
    assert!(
        matches!(result, Err(RegistryError::AlreadyRegistered)),
        "second register with identical (name, source_id) must reject; got: {result:?}",
    );
}
