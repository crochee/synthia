//! TDD: atomic hot-swap test for `ProviderRegistry` v2's `replace_source`.
//!
//! RED phase: written first; v2 module does not exist yet.

use std::sync::Arc;

use async_trait::async_trait;
use synthia_provider::{
    CompletionRequest,
    CompletionResponse,
    ModelConfig,
    ModelProvider,
    ProviderConfig,
    registry::v2::{ProviderRegistry, SourceId},
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
async fn replace_source_atomic_swap() {
    let registry = ProviderRegistry::new();
    let p: Arc<dyn ModelProvider> = Arc::new(MockProvider);

    // Seed: two providers under ext-a, plus one under ext-b to confirm
    // source isolation across the swap.
    registry
        .register("gpt-4", p.clone(), SourceId("ext-a".to_string()))
        .await
        .expect("register gpt-4/ext-a");
    registry
        .register("claude", p.clone(), SourceId("ext-a".to_string()))
        .await
        .expect("register claude/ext-a");
    registry
        .register("other", p.clone(), SourceId("ext-b".to_string()))
        .await
        .expect("register other/ext-b");

    // Atomic swap for ext-a → one new entry.
    let new_set: Vec<(String, Arc<dyn ModelProvider>)> =
        vec![("gpt-4-mini".to_string(), p.clone())];
    let removed = registry
        .replace_source(SourceId("ext-a".to_string()), new_set)
        .await
        .expect("replace_source should succeed");

    // Returned count is the number of NEW entries inserted (1).
    assert_eq!(removed, 1, "returned count is the size of the new set");

    // ext-a entries are gone.
    assert!(
        registry.get("gpt-4").await.is_none(),
        "gpt-4 (was ext-a) must be removed",
    );
    assert!(
        registry.get("claude").await.is_none(),
        "claude (was ext-a) must be removed",
    );
    // New entry is present.
    assert!(
        registry.get("gpt-4-mini").await.is_some(),
        "gpt-4-mini (inserted by swap) must be present",
    );
    // ext-b entry is untouched by an ext-a swap.
    assert!(
        registry.get("other").await.is_some(),
        "other (ext-b) must survive an ext-a swap",
    );
}
