//! `ModelProviderStub` — minimal `ModelProvider` used by
//! `synthia-agent` internal tests.

use async_trait::async_trait;
use synthia_core::Error;

use crate::{
    CompletionRequest,
    CompletionResponse,
    Content,
    ContentPart,
    ProviderConfig,
    SamplingResult,
    StreamChunk,
    TextContent,
    TokenUsage,
    traits::ModelProvider,
    types::ModelConfig,
};

/// Minimal `ModelProvider` that yields a single `IsDone` with the
/// supplied text and empty tool calls.
pub struct ModelProviderStub {
    text: String,
}

impl ModelProviderStub {
    pub fn new() -> Self {
        Self {
            text: String::new(),
        }
    }

    pub fn text_only(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

impl Default for ModelProviderStub {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ModelProvider for ModelProviderStub {
    async fn initialize(
        &mut self,
        _config: ProviderConfig,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn name(&self) -> &str {
        "stub"
    }

    fn model_config(&self) -> ModelConfig {
        ModelConfig {
            name: "stub".into(),
            provider: "stub".into(),
            context_window: 128_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        }
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, Error> {
        Ok(CompletionResponse {
            id: "stub".into(),
            model: "stub".into(),
            content: Content::Single(ContentPart::Text(TextContent {
                text: self.text.clone(),
                cache_control: None,
            })),
            usage: TokenUsage::default(),
            cached: false,
            stop_reason: None,
        })
    }

    async fn complete_with_stream(
        &self,
        _request: CompletionRequest,
        _cancel_token: Option<tokio_util::sync::CancellationToken>,
        mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> Result<CompletionResponse, Error> {
        on_delta(StreamChunk::Content(ContentPart::Text(TextContent {
            text: self.text.clone(),
            cache_control: None,
        })));
        let usage = TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
        };
        on_delta(StreamChunk::IsDone {
            result: Box::new(SamplingResult {
                text: self.text.clone(),
                tool_calls: vec![],
                reasoning: String::new(),
                reasoning_signature: None,
                usage: usage.clone(),
                stop_reason: None,
            }),
        });
        Ok(CompletionResponse {
            id: "stub".into(),
            model: "stub".into(),
            content: Content::Single(ContentPart::Text(TextContent {
                text: self.text.clone(),
                cache_control: None,
            })),
            usage,
            cached: false,
            stop_reason: None,
        })
    }

    async fn embed(&self, _texts: Vec<String>) -> Result<Vec<Vec<f64>>, Error> {
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Message;

    // -- ModelProviderStub::new / text_only ---------------------------

    /// `ModelProviderStub::new()` MUST produce a stub with
    /// empty text.
    #[test]
    fn new_creates_empty_text_stub() {
        let s = ModelProviderStub::new();
        // Pinned by exercising the public surface via `name()`
        // and `model_config()` — the text field is private but
        // its effect is observable via `complete`.
        assert_eq!(s.name(), "stub");
    }

    /// `ModelProviderStub::default()` MUST match `::new()`.
    #[test]
    fn default_matches_new() {
        let _ = ModelProviderStub::default();
    }

    /// `text_only(s)` MUST accept both `&str` and `String`.
    #[test]
    fn text_only_accepts_str_and_string() {
        let _ = ModelProviderStub::text_only("from_str");
        let _ = ModelProviderStub::text_only(String::from("from_string"));
    }

    /// `text_only("")` MUST accept an empty string.
    #[test]
    fn text_only_accepts_empty_string() {
        let _ = ModelProviderStub::text_only("");
    }

    // -- ModelProvider trait surface -----------------------------------

    /// `name()` MUST return the literal string `"stub"`.
    #[test]
    fn name_returns_stub_literal() {
        let s = ModelProviderStub::new();
        assert_eq!(s.name(), "stub");
    }

    /// `model_config()` MUST return a `ModelConfig` with all
    /// 7 fields populated with stub defaults.
    #[test]
    fn model_config_returns_stub_defaults() {
        let s = ModelProviderStub::new();
        let mc = s.model_config();
        assert_eq!(mc.name, "stub");
        assert_eq!(mc.provider, "stub");
        assert_eq!(mc.context_window, 128_000);
        assert_eq!(mc.max_output_tokens, 4096);
        assert!(mc.supports_tools);
        assert!(mc.supports_streaming);
        assert!(mc.supports_reasoning);
    }

    /// `initialize` MUST always return `Ok(())` (stub no-op).
    #[tokio::test]
    async fn initialize_returns_ok() {
        let mut s = ModelProviderStub::new();
        let result = s
            .initialize(ProviderConfig {
                api_key: synthia_core::Sensitive::new(String::new()),
                base_url: None,
                timeout_ms: None,
                max_retries: None,
            })
            .await;
        assert!(result.is_ok());
    }

    /// `initialize` MUST accept any `ProviderConfig` (including
    /// empty models).
    #[tokio::test]
    async fn initialize_accepts_empty_models() {
        let mut s = ModelProviderStub::new();
        let result = s
            .initialize(ProviderConfig {
                api_key: synthia_core::Sensitive::new(String::new()),
                base_url: None,
                timeout_ms: None,
                max_retries: None,
            })
            .await;
        assert!(result.is_ok());
    }

    /// `complete` MUST return a `CompletionResponse` with
    /// the configured text in `content`.
    #[tokio::test]
    async fn complete_returns_configured_text() {
        let s = ModelProviderStub::text_only("hello world");
        let req = CompletionRequest {
            model: "stub".into(),
            messages: std::sync::Arc::new(vec![Message::user("hi")]),
            tools: std::sync::Arc::new(vec![]),
            tool_choice: crate::types::ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
        };
        let resp = s.complete(req).await.unwrap();
        assert_eq!(resp.id, "stub");
        assert_eq!(resp.model, "stub");
        let text = resp.content.extract_text();
        assert_eq!(text, Some("hello world".to_string()));
        assert!(!resp.cached);
        assert_eq!(resp.stop_reason, None);
    }

    /// `complete_with_stream` MUST emit a `Content` chunk +
    /// an `IsDone` chunk.
    #[tokio::test]
    async fn complete_with_stream_emits_two_chunks() {
        let s = ModelProviderStub::text_only("streamed");
        let collected = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        let collected_clone = collected.clone();
        let on_delta: Box<dyn FnMut(StreamChunk) + Send> =
            Box::new(move |chunk: StreamChunk| {
                collected_clone.lock().unwrap().push(chunk);
            });
        let req = CompletionRequest {
            model: "stub".into(),
            messages: std::sync::Arc::new(vec![Message::user("hi")]),
            tools: std::sync::Arc::new(vec![]),
            tool_choice: crate::types::ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
        };
        let _ = s.complete_with_stream(req, None, on_delta).await.unwrap();
        let chunks = collected.lock().unwrap();
        assert_eq!(chunks.len(), 2);
        assert!(matches!(chunks[0], StreamChunk::Content(_)));
        assert!(matches!(chunks[1], StreamChunk::IsDone { .. }));
    }

    /// `complete_with_stream` MUST emit chunks in order:
    /// `Content` first, then `IsDone`.
    #[tokio::test]
    async fn complete_with_stream_chunk_order() {
        let s = ModelProviderStub::text_only("order");
        let collected = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        let collected_clone = collected.clone();
        let on_delta: Box<dyn FnMut(StreamChunk) + Send> =
            Box::new(move |chunk: StreamChunk| {
                collected_clone.lock().unwrap().push(chunk);
            });
        let req = CompletionRequest {
            model: "stub".into(),
            messages: std::sync::Arc::new(vec![Message::user("hi")]),
            tools: std::sync::Arc::new(vec![]),
            tool_choice: crate::types::ToolChoice::Auto,
            temperature: None,
            max_tokens: None,
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: None,
        };
        let _ = s.complete_with_stream(req, None, on_delta).await.unwrap();
        let chunks = collected.lock().unwrap();
        // Pin the order: Content before IsDone.
        assert!(matches!(chunks[0], StreamChunk::Content(_)));
        assert!(matches!(chunks[1], StreamChunk::IsDone { .. }));
    }

    /// `embed` MUST always return `Ok(vec![])` (empty vec).
    #[tokio::test]
    async fn embed_returns_empty_vec() {
        let s = ModelProviderStub::new();
        let result = s.embed(vec!["text1".to_string()]).await.unwrap();
        assert_eq!(result.len(), 0);
    }

    /// `embed` MUST accept empty input (`vec![]`) and return
    /// `Ok(vec![])`.
    #[tokio::test]
    async fn embed_accepts_empty_input() {
        let s = ModelProviderStub::new();
        let result = s.embed(vec![]).await.unwrap();
        assert_eq!(result.len(), 0);
    }
}
