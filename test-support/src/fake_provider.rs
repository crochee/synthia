use std::sync::Arc;

use async_trait::async_trait;
use synthia_core::Error;
use synthia_provider::*;
use tokio::sync::Mutex;

/// FakeProvider that supports both complete() and stream() methods.
///
/// For streaming, provide `stream_chunks` which is a Vec<Vec<StreamChunk>>.
/// Each inner Vec is returned on a successive call to stream().
/// This allows tests to simulate multi-turn tool-use scenarios.
#[derive(Debug)]
pub struct FakeProvider {
    pub responses: Vec<CompletionResponse>,
    pub call_count: std::sync::atomic::AtomicUsize,
    pub stream_chunks: Arc<Mutex<Vec<Vec<StreamChunk>>>>,
    pub stream_call_count: std::sync::atomic::AtomicUsize,
}

impl FakeProvider {
    pub fn new(responses: Vec<CompletionResponse>) -> Self {
        Self {
            responses,
            call_count: std::sync::atomic::AtomicUsize::new(0),
            stream_chunks: Arc::new(Mutex::new(Vec::new())),
            stream_call_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub fn with_response(response: CompletionResponse) -> Self {
        Self::new(vec![response])
    }

    /// Configure streaming chunks. Each call to stream() will return
    /// the next entry from this vector as an async stream.
    pub fn with_stream_chunks(mut self, chunks: Vec<Vec<StreamChunk>>) -> Self {
        self.stream_chunks = Arc::new(Mutex::new(chunks));
        self
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
            Err(Error::RateLimited(None))
        }
    }

    async fn embed(&self, _texts: Vec<String>) -> Result<Vec<Vec<f64>>, Error> {
        // Return dummy embeddings for testing
        Ok(vec![vec![0.0; 1536]; _texts.len()])
    }
}
