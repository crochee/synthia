//! ModelProvider trait definition

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use synthia_core::Error;
use tokio_util::sync::CancellationToken;

use crate::types::{
    CompletionRequest,
    CompletionResponse,
    Content,
    ContentPart,
    ModelConfig,
    ProviderConfig,
    SamplingResult,
    StreamChunk,
};

pub type StreamResult =
    Pin<Box<dyn Stream<Item = Result<StreamChunk, Error>> + Send>>;

/// Convert a `CompletionResponse` (provider view) into a `SamplingResult` (agent view).
/// Best-effort: extracts text/tool_calls/reasoning/usage from `content`.
pub fn completion_to_sampling(resp: &CompletionResponse) -> SamplingResult {
    let mut text_parts: Vec<String> = Vec::new();
    let mut tool_calls = Vec::new();
    let mut reasoning_parts: Vec<String> = Vec::new();
    let parts: Vec<&ContentPart> = match &resp.content {
        Content::Single(p) => vec![p],
        Content::Multi(ps) => ps.iter().collect(),
    };
    for p in parts {
        match p {
            ContentPart::Text(t) => text_parts.push(t.text.clone()),
            ContentPart::ToolUse(t) => tool_calls.push(t.clone()),
            ContentPart::Reasoning(t) => reasoning_parts.push(t.text.clone()),
            _ => {}
        }
    }
    SamplingResult {
        text: text_parts.join(""),
        tool_calls,
        reasoning: reasoning_parts.join(""),
        usage: resp.usage.clone(),
    }
}

#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Validate credentials, preload model list, warm connection pool
    /// Called at agent startup by the interface layer
    async fn initialize(&mut self, config: ProviderConfig)
    -> Result<(), Error>;

    fn name(&self) -> &str;

    /// Return model capabilities configuration
    fn model_config(&self) -> ModelConfig;

    /// Whether this provider supports inline `cache_control` hints
    /// (Anthropic, Bedrock Converse). Providers using implicit prefix
    /// caching (OpenAI) return `false`.
    fn supports_inline_cache_hints(&self) -> bool {
        false
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, Error>;

    /// Callback-based streaming completion.
    ///
    /// `cancel_token` allows the caller to abort an in-flight stream
    /// (5-second default grace is enforced by the Anthropic implementation
    /// to flush the underlying HTTP body). The default implementation
    /// calls `complete()` and emits a single `StreamChunk::IsDone` carrying
    /// the resulting `SamplingResult`. Providers that support real
    /// streaming may override this to emit incremental deltas
    /// (`Content` / `ToolCallDelta` / `Usage` / etc.) and the terminal
    /// `IsDone` chunk. See `StreamChunk` for the event vocabulary.
    async fn complete_with_stream(
        &self,
        request: CompletionRequest,
        cancel_token: Option<CancellationToken>,
        mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> Result<CompletionResponse, Error> {
        // The default impl ignores `cancel_token` (it does a single
        // `complete()` call which is bounded by HTTP timeouts). Real
        // streaming overrides must honour it.
        let _ = cancel_token;
        let response = self.complete(request).await?;
        let sampling = completion_to_sampling(&response);
        on_delta(StreamChunk::IsDone {
            result: Box::new(sampling),
        });
        Ok(response)
    }

    /// Generate dense vector embeddings for a list of text strings.
    /// Returns a list of embedding vectors (Vec<Vec<f64>>).
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f64>>, Error>;
}
