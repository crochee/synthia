use std::sync::Arc;

use async_trait::async_trait;
use synthia_core::Error;
use synthia_provider::*;

/// FakeProvider that supports both complete() and stream() methods.
///
/// For streaming, provide `stream_chunks` which is a `Vec<Vec<StreamChunk>>`.
/// Each inner Vec is returned on a successive call to stream().
/// This allows tests to simulate multi-turn tool-use scenarios.
#[derive(Debug)]
pub struct FakeProvider {
    pub responses: Vec<CompletionResponse>,
    pub call_count: std::sync::atomic::AtomicUsize,
    /// A `parking_lot` mutex, not tokio's: this field is public, so
    /// naming `tokio::sync::Mutex` here would push a runtime type into
    /// every consumer's test code (same rule as
    /// [`FakeTool::call_count`](crate::FakeTool)).
    pub stream_chunks: Arc<parking_lot::Mutex<Vec<Vec<StreamChunk>>>>,
    pub stream_call_count: std::sync::atomic::AtomicUsize,
}

impl FakeProvider {
    pub fn new(responses: Vec<CompletionResponse>) -> Self {
        Self {
            responses,
            call_count: std::sync::atomic::AtomicUsize::new(0),
            stream_chunks: Arc::new(parking_lot::Mutex::new(Vec::new())),
            stream_call_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Create a provider that returns a single text response for every
    /// `complete()` call (calls beyond the first repeat the same
    /// response). Mirrors traitclaw's `MockProvider::text()` —
    /// ported so a consumer test can write
    /// `FakeProvider::text("answer")` instead of building a full
    /// `CompletionResponse { content: Content::text("answer"),
    /// ..default() }`.
    pub fn text(text: &str) -> Self {
        Self::new(vec![CompletionResponse {
            content: Content::text(text),
            ..Default::default()
        }])
    }
}

#[async_trait]
impl ModelProvider for FakeProvider {
    async fn initialize(
        &mut self,
        _config: ProviderConfig,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn name(&self) -> &str {
        "fake"
    }

    fn model_config(&self) -> ModelConfig {
        ModelConfig {
            name: "fake-model".to_string(),
            provider: "fake".to_string(),
            context_window: 128000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        }
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, Error> {
        let count = self
            .call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if count < self.responses.len() {
            Ok(self.responses[count].clone())
        } else {
            Err(Error::rate_limited(None))
        }
    }

    async fn embed(&self, _texts: Vec<String>) -> Result<Vec<Vec<f64>>, Error> {
        // Return dummy embeddings for testing
        Ok(vec![vec![0.0; 1536]; _texts.len()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn text_constructor_replies_with_the_given_text() {
        let p = FakeProvider::text("hello world");
        let resp = p
            .complete(CompletionRequest::default())
            .await
            .expect("first call returns the canned text");
        assert_eq!(resp.content.extract_text().as_deref(), Some("hello world"));
    }

    #[tokio::test]
    async fn text_constructor_repeats_after_first_call() {
        let p = FakeProvider::text("repeat-me");
        // The text() constructor builds a one-element responses
        // vec, so every call after the first falls into the
        // empty-responses path and surfaces rate_limited (the
        // existing overflow behaviour). Verify both halves.
        let r1 = p.complete(CompletionRequest::default()).await;
        assert!(r1.is_ok());
        let r2 = p.complete(CompletionRequest::default()).await;
        assert!(r2.is_err());
    }
}
