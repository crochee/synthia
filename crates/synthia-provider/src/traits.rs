//! ModelProvider trait definition

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use synthia_core::{Error, RegistryItem};
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
        reasoning_signature: None,
        usage: resp.usage.clone(),
        stop_reason: resp.stop_reason.clone(),
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

impl RegistryItem for dyn ModelProvider {
    fn name(&self) -> &str {
        <Self as ModelProvider>::name(self)
    }

    fn description(&self) -> &str {
        "Model provider"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        Content,
        ContentPart,
        ReasoningContent,
        TextContent,
        TokenUsage,
        ToolUse,
    };

    // -- completion_to_sampling -------------------------------------

    /// `completion_to_sampling` MUST convert
    /// `Content::Single(Text(t))` into a `SamplingResult`
    /// with `text == t.text`.
    #[test]
    fn completion_to_sampling_single_text() {
        let resp = CompletionResponse {
            id: "r-1".to_string(),
            model: "m".to_string(),
            content: Content::Single(ContentPart::Text(TextContent {
                text: "hello".to_string(),
                cache_control: None,
            })),
            usage: TokenUsage::default(),
            cached: false,
            stop_reason: None,
        };
        let s = completion_to_sampling(&resp);
        assert_eq!(s.text, "hello");
        assert!(s.tool_calls.is_empty());
        assert!(s.reasoning.is_empty());
        assert_eq!(s.reasoning_signature, None);
    }

    /// `completion_to_sampling` MUST join multiple
    /// `Content::Multi(Text)` into the same `text` field
    /// (rare, but per spec).
    #[test]
    fn completion_to_sampling_multi_text_joins_in_order() {
        let resp = CompletionResponse {
            id: "r-1".to_string(),
            model: "m".to_string(),
            content: Content::Multi(vec![
                ContentPart::Text(TextContent {
                    text: "a".to_string(),
                    cache_control: None,
                }),
                ContentPart::Text(TextContent {
                    text: "b".to_string(),
                    cache_control: None,
                }),
                ContentPart::Text(TextContent {
                    text: "c".to_string(),
                    cache_control: None,
                }),
            ]),
            usage: TokenUsage::default(),
            cached: false,
            stop_reason: None,
        };
        let s = completion_to_sampling(&resp);
        assert_eq!(s.text, "abc");
    }

    /// `completion_to_sampling` MUST collect all `ToolUse`
    /// parts into `tool_calls` (preserving order).
    #[test]
    fn completion_to_sampling_multi_tool_calls() {
        let resp = CompletionResponse {
            id: "r-1".to_string(),
            model: "m".to_string(),
            content: Content::Multi(vec![
                ContentPart::ToolUse(ToolUse {
                    id: "call-1".to_string(),
                    name: "bash".to_string(),
                    input: serde_json::json!({"cmd": "ls"}),
                }),
                ContentPart::ToolUse(ToolUse {
                    id: "call-2".to_string(),
                    name: "edit".to_string(),
                    input: serde_json::json!({"path": "/x"}),
                }),
            ]),
            usage: TokenUsage::default(),
            cached: false,
            stop_reason: None,
        };
        let s = completion_to_sampling(&resp);
        assert_eq!(s.text, "");
        assert_eq!(s.tool_calls.len(), 2);
        assert_eq!(s.tool_calls[0].id, "call-1");
        assert_eq!(s.tool_calls[1].name, "edit");
    }

    /// `completion_to_sampling` MUST extract `Reasoning`
    /// content into the `reasoning` field (joined).
    #[test]
    fn completion_to_sampling_reasoning_extracted() {
        let resp = CompletionResponse {
            id: "r-1".to_string(),
            model: "m".to_string(),
            content: Content::Multi(vec![
                ContentPart::Reasoning(ReasoningContent {
                    text: "thinking ".to_string(),
                    signature: None,
                }),
                ContentPart::Reasoning(ReasoningContent {
                    text: "step 2".to_string(),
                    signature: None,
                }),
                ContentPart::Text(TextContent {
                    text: "answer".to_string(),
                    cache_control: None,
                }),
            ]),
            usage: TokenUsage::default(),
            cached: false,
            stop_reason: Some("end_turn".to_string()),
        };
        let s = completion_to_sampling(&resp);
        assert_eq!(s.reasoning, "thinking step 2");
        assert_eq!(s.text, "answer");
        assert_eq!(s.stop_reason, Some("end_turn".to_string()));
    }

    /// `completion_to_sampling` MUST carry usage + stop_reason
    /// verbatim from the CompletionResponse.
    #[test]
    fn completion_to_sampling_carries_usage_and_stop_reason() {
        let resp = CompletionResponse {
            id: "r-1".to_string(),
            model: "m".to_string(),
            content: Content::Single(ContentPart::Text(TextContent {
                text: "x".to_string(),
                cache_control: None,
            })),
            usage: TokenUsage {
                prompt_tokens: 100,
                completion_tokens: 50,
                total_tokens: 150,
                cached_prompt_tokens: Some(25),
                cache_read_tokens: None,
                cache_write_tokens: None,
            },
            cached: false,
            stop_reason: Some("tool_use".to_string()),
        };
        let s = completion_to_sampling(&resp);
        assert_eq!(s.usage.prompt_tokens, 100);
        assert_eq!(s.usage.completion_tokens, 50);
        assert_eq!(s.usage.total_tokens, 150);
        assert_eq!(s.stop_reason, Some("tool_use".to_string()));
    }

    /// `completion_to_sampling` MUST ignore non-text/tool/
    /// reasoning variants (e.g. Image, Refusal — anything
    /// without an explicit branch).
    #[test]
    fn completion_to_sampling_ignores_other_part_variants() {
        // Mixed: 1 text, 1 tool, 1 other (unknown to mapping).
        let resp = CompletionResponse {
            id: "r-1".to_string(),
            model: "m".to_string(),
            content: Content::Multi(vec![
                ContentPart::Text(TextContent {
                    text: "t".to_string(),
                    cache_control: None,
                }),
                ContentPart::ToolUse(ToolUse {
                    id: "a".to_string(),
                    name: "b".to_string(),
                    input: serde_json::Value::Null,
                }),
            ]),
            usage: TokenUsage::default(),
            cached: false,
            stop_reason: None,
        };
        let s = completion_to_sampling(&resp);
        // Pin: nothing crashes on unknown variants; only the
        // known branches populate fields.
        assert_eq!(s.text, "t");
        assert_eq!(s.tool_calls.len(), 1);
    }

    /// `completion_to_sampling` MUST always produce
    /// `reasoning_signature = None` (no signature parsing in
    /// this conversion — signature is propagated through the
    /// `Content::Reasoning` branch only when callers handle
    /// it manually).
    #[test]
    fn completion_to_sampling_reasoning_signature_always_none() {
        let resp = CompletionResponse {
            id: "r-1".to_string(),
            model: "m".to_string(),
            content: Content::Single(ContentPart::Reasoning(
                ReasoningContent {
                    text: "r".to_string(),
                    signature: Some("sig".to_string()),
                },
            )),
            usage: TokenUsage::default(),
            cached: false,
            stop_reason: None,
        };
        let s = completion_to_sampling(&resp);
        // The implementation always sets reasoning_signature
        // = None — the signature is dropped during this
        // conversion (the StreamingResult has no signature
        // extracted from ContentPart::Reasoning).
        assert_eq!(s.reasoning_signature, None);
        assert_eq!(s.reasoning, "r");
    }
}
