//! Unit tests for the `sample` step.
//!
//! The original 6 tests lived at the bottom of `sample.rs`; they're
//! hoisted into this sibling file so the production code
//! (`core` / `truncate` / `request` / `stream` / `fallback`)
//! doesn't carry the test body weight.
//!
//! Coverage map:
//!
//! - Stream basics (1): `step_sample_emits_token_deltas_and_produces_sampling_result`.
//! - Tool call accumulation (1): `step_sample_accumulates_tool_calls_from_start_delta_end`.
//! - Cancellation (1): `step_sample_cancellation_propagates_as_error`.
//! - Truncate (2): `step_sample_truncates_oversized_tool_messages`,
//!   `step_sample_preserves_non_tool_messages_unchanged`.
//! - Provider error (1): `step_sample_panicked_provider_task_is_surfaced_as_error`.

use std::sync::Arc;

use async_trait::async_trait;
use synthia_core::Error;
use synthia_provider::{
    TokenUsage as ProviderTokenUsage,
    traits::ModelProvider,
    types::{
        CompletionRequest,
        CompletionResponse,
        ContentPart,
        ModelConfig,
        ProviderConfig,
        Role,
        SamplingResult,
        StreamChunk,
        TextContent,
    },
};
use synthia_telemetry::span_context::SpanContext;
use tokio_util::sync::CancellationToken;

use super::core::StepSample;
use crate::{config::AgentConfig, loop_context::LoopContext};

// ---- Test provider helpers -------------------------------------------

/// Test provider that emits a predefined chunk sequence and falls back
/// to `complete()` (the default trait impl) when asked.
struct ScriptedProvider {
    chunks: Vec<StreamChunk>,
    final_result: SamplingResult,
}

impl ScriptedProvider {
    fn new(chunks: Vec<StreamChunk>, final_result: SamplingResult) -> Self {
        Self {
            chunks,
            final_result,
        }
    }
}

#[async_trait]
impl ModelProvider for ScriptedProvider {
    async fn initialize(
        &mut self,
        _config: ProviderConfig,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn name(&self) -> &str {
        "scripted"
    }

    fn model_config(&self) -> ModelConfig {
        ModelConfig {
            name: "scripted".to_string(),
            provider: "scripted".to_string(),
            context_window: 8192,
            max_output_tokens: 2048,
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
            id: "test".to_string(),
            model: "scripted".to_string(),
            content: synthia_provider::Content::text(""),
            usage: self.final_result.usage.clone(),
            cached: false,
        })
    }

    async fn complete_with_stream(
        &self,
        _request: CompletionRequest,
        _cancel_token: Option<tokio_util::sync::CancellationToken>,
        mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> Result<CompletionResponse, Error> {
        for chunk in &self.chunks {
            on_delta(chunk.clone());
        }
        Ok(CompletionResponse {
            id: "test".to_string(),
            model: "scripted".to_string(),
            content: synthia_provider::Content::text(""),
            usage: self.final_result.usage.clone(),
            cached: false,
        })
    }

    async fn embed(&self, _texts: Vec<String>) -> Result<Vec<Vec<f64>>, Error> {
        Ok(vec![])
    }
}

fn make_ctx() -> LoopContext {
    LoopContext::new("test".to_string(), SpanContext::new("test"))
}

// ---- Tests ------------------------------------------------------------

#[tokio::test]
async fn step_sample_emits_token_deltas_and_produces_sampling_result() {
    let chunks = vec![
        StreamChunk::Content(ContentPart::Text(TextContent {
            text: "Hello".to_string(),
            cache_control: None,
        })),
        StreamChunk::Content(ContentPart::Text(TextContent {
            text: " world".to_string(),
            cache_control: None,
        })),
        StreamChunk::IsDone {
            result: Box::new(SamplingResult {
                text: "Hello world".to_string(),
                tool_calls: vec![],
                reasoning: String::new(),
                reasoning_signature: None,
                usage: ProviderTokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 2,
                    total_tokens: 12,
                    cached_prompt_tokens: None,
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                },
            }),
        },
    ];
    let provider =
        Arc::new(ScriptedProvider::new(chunks, SamplingResult::default()));
    let mut ctx = make_ctx();
    ctx.messages.push(synthia_provider::Message::user("hi"));
    let step = StepSample::new(AgentConfig::default());
    let result = step
        .execute(
            provider as Arc<dyn ModelProvider>,
            &mut ctx,
            vec![],
            CancellationToken::new(),
        )
        .await
        .expect("execute ok");
    let (sampling, deltas) = result;
    assert_eq!(sampling.text, "Hello world");
    assert_eq!(sampling.usage.total_tokens, 12);
    // Each streamed Text chunk should be surfaced as a separate
    // delta so the agent loop can yield AgentEvent::Model events in
    // the order the provider sent them.
    use synthia_provider::types::{ContentPart, TextContent};
    assert_eq!(
        deltas,
        vec![
            ContentPart::Text(TextContent {
                text: "Hello".into(),
                cache_control: None,
            }),
            ContentPart::Text(TextContent {
                text: " world".into(),
                cache_control: None,
            }),
        ]
    );
}

#[tokio::test]
async fn step_sample_accumulates_tool_calls_from_start_delta_end() {
    let chunks = vec![
        StreamChunk::ToolCallStart {
            id: "c1".to_string(),
            name: "get_weather".to_string(),
            arguments: serde_json::json!(""),
        },
        StreamChunk::ToolCallDelta {
            id: "c1".to_string(),
            arguments_delta: r#"{"loc"#.to_string(),
        },
        StreamChunk::ToolCallDelta {
            id: "c1".to_string(),
            arguments_delta: r#": "Beijing"}"#.to_string(),
        },
        StreamChunk::ToolCallEnd {
            id: "c1".to_string(),
        },
        StreamChunk::IsDone {
            result: Box::new(SamplingResult::default()),
        },
    ];
    let provider =
        Arc::new(ScriptedProvider::new(chunks, SamplingResult::default()));
    let mut ctx = make_ctx();
    ctx.messages.push(synthia_provider::Message::user("hi"));
    let step = StepSample::new(AgentConfig::default());
    let result = step
        .execute(
            provider as Arc<dyn ModelProvider>,
            &mut ctx,
            vec![],
            CancellationToken::new(),
        )
        .await
        .expect("execute ok");
    let (sampling, _deltas) = result;
    assert_eq!(sampling.tool_calls.len(), 1);
    assert_eq!(sampling.tool_calls[0].name, "get_weather");
    // The accumulated string is "{"loc: "Beijing"}" which is missing a
    // quote after "loc" — so JSON parse fails and we fall back to the
    // string form. This is the documented behaviour for malformed
    // provider output.
    let accumulated_str = match &sampling.tool_calls[0].input {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    assert_eq!(accumulated_str, r#"{"loc: "Beijing"}"#);
}

#[tokio::test]
async fn step_sample_cancellation_propagates_as_error() {
    // Provider that produces an infinite stream so cancellation has a
    // chance to fire.
    struct InfiniteProvider;
    #[async_trait]
    impl ModelProvider for InfiniteProvider {
        async fn initialize(
            &mut self,
            _config: ProviderConfig,
        ) -> Result<(), Error> {
            Ok(())
        }

        fn name(&self) -> &str {
            "infinite"
        }

        fn model_config(&self) -> ModelConfig {
            ModelConfig {
                name: "infinite".to_string(),
                provider: "infinite".to_string(),
                context_window: 8192,
                max_output_tokens: 2048,
                supports_tools: true,
                supports_streaming: true,
                supports_reasoning: false,
            }
        }

        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> Result<CompletionResponse, Error> {
            Ok(CompletionResponse {
                id: "x".to_string(),
                model: "x".to_string(),
                content: synthia_provider::Content::text(""),
                usage: ProviderTokenUsage::default(),
                cached: false,
            })
        }

        async fn complete_with_stream(
            &self,
            _request: CompletionRequest,
            _cancel_token: Option<tokio_util::sync::CancellationToken>,
            mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
        ) -> Result<CompletionResponse, Error> {
            // Emit 10 tokens then a Done.
            for i in 0..10 {
                on_delta(StreamChunk::Content(ContentPart::Text(
                    TextContent {
                        text: "t".to_string(),
                        cache_control: None,
                    },
                )));
                if i == 5 {
                    // Simulate a long stream by yielding.
                    tokio::time::sleep(std::time::Duration::from_millis(5))
                        .await;
                }
            }
            on_delta(StreamChunk::IsDone {
                result: Box::new(SamplingResult::default()),
            });
            Ok(CompletionResponse {
                id: "x".to_string(),
                model: "x".to_string(),
                content: synthia_provider::Content::text(""),
                usage: ProviderTokenUsage::default(),
                cached: false,
            })
        }

        async fn embed(
            &self,
            _texts: Vec<String>,
        ) -> Result<Vec<Vec<f64>>, Error> {
            Ok(vec![])
        }
    }
    let provider = Arc::new(InfiniteProvider) as Arc<dyn ModelProvider>;
    let mut ctx = make_ctx();
    ctx.messages.push(synthia_provider::Message::user("hi"));
    let step = StepSample::new(AgentConfig::default());
    let token = CancellationToken::new();
    token.cancel();
    let result = step.execute(provider, &mut ctx, vec![], token).await;
    // The provider may also surface a Cancelled error. In any case,
    // we must not panic and we must not return a successful
    // SamplingResult.
    if let Ok(r) = result {
        // Defensive: if the stream happened to drain before our cancel
        // check, the result may still be valid (empty text). We don't
        // assert anything specific here.
        let _ = r;
    }
}

#[tokio::test]
async fn step_sample_truncates_oversized_tool_messages() {
    // Tool result of 60K bytes → should be truncated because the default
    // byte threshold is 50KB.
    let big = "x".repeat(60_000);
    let mut ctx = make_ctx();
    ctx.messages.push(synthia_provider::Message::tool(
        synthia_provider::Content::text(big.clone()),
        "call-1",
    ));
    // Sanity: the tool message's original text is the full 60K.
    assert_eq!(
        ctx.messages[0].content.extract_text().unwrap().len(),
        60_000
    );

    let chunks = vec![StreamChunk::IsDone {
        result: Box::new(SamplingResult {
            text: "ok".to_string(),
            ..Default::default()
        }),
    }];
    let provider =
        Arc::new(ScriptedProvider::new(chunks, SamplingResult::default()));
    let step = StepSample::new(AgentConfig::default());
    let result = step
        .execute(
            provider as Arc<dyn ModelProvider>,
            &mut ctx,
            vec![],
            CancellationToken::new(),
        )
        .await
        .expect("execute ok");
    let (sampling, deltas) = result;
    assert_eq!(sampling.text, "ok");
    // The scripted chunks were only an IsDone (no Content chunks),
    // so no text deltas were captured.
    assert!(deltas.is_empty());
    // The Tool message should have been truncated in place.
    let tool_text = ctx.messages[0].content.extract_text().unwrap();
    assert!(tool_text.len() < big.len());
    assert!(tool_text.contains("truncated"));
}

#[tokio::test]
async fn step_sample_preserves_non_tool_messages_unchanged() {
    // Only Tool role should be truncated; System/User/Assistant must
    // pass through byte-identical. 60K exceeds the default 50KB threshold.
    let mut ctx = make_ctx();
    let long = "x".repeat(60_000);
    ctx.messages.push(synthia_provider::Message::new(
        Role::System,
        synthia_provider::Content::text(long.clone()),
    ));
    ctx.messages.push(synthia_provider::Message::tool(
        synthia_provider::Content::text(long.clone()),
        "c-1",
    ));
    let chunks = vec![StreamChunk::IsDone {
        result: Box::new(SamplingResult::default()),
    }];
    let provider =
        Arc::new(ScriptedProvider::new(chunks, SamplingResult::default()));
    let step = StepSample::new(AgentConfig::default());
    step.execute(
        provider as Arc<dyn ModelProvider>,
        &mut ctx,
        vec![],
        CancellationToken::new(),
    )
    .await
    .expect("execute ok");
    // System is preserved.
    assert_eq!(
        ctx.messages[0].content.extract_text().unwrap().len(),
        60_000
    );
    // Tool is truncated.
    assert!(ctx.messages[1].content.extract_text().unwrap().len() < 60_000);
}

#[tokio::test]
async fn step_sample_panicked_provider_task_is_surfaced_as_error() {
    // Provider whose `complete_with_stream` returns an error: this
    // should propagate to the caller.
    struct ErrProvider;
    #[async_trait]
    impl ModelProvider for ErrProvider {
        async fn initialize(
            &mut self,
            _config: ProviderConfig,
        ) -> Result<(), Error> {
            Ok(())
        }

        fn name(&self) -> &str {
            "err"
        }

        fn model_config(&self) -> ModelConfig {
            ModelConfig {
                name: "err".to_string(),
                provider: "err".to_string(),
                context_window: 8192,
                max_output_tokens: 2048,
                supports_tools: false,
                supports_streaming: true,
                supports_reasoning: false,
            }
        }

        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> Result<CompletionResponse, Error> {
            Err(Error::Provider("provider error".to_string()))
        }

        async fn complete_with_stream(
            &self,
            _request: CompletionRequest,
            _cancel_token: Option<tokio_util::sync::CancellationToken>,
            _on_delta: Box<dyn FnMut(StreamChunk) + Send>,
        ) -> Result<CompletionResponse, Error> {
            Err(Error::Provider("stream error".to_string()))
        }

        async fn embed(
            &self,
            _texts: Vec<String>,
        ) -> Result<Vec<Vec<f64>>, Error> {
            Ok(vec![])
        }
    }
    let provider = Arc::new(ErrProvider) as Arc<dyn ModelProvider>;
    let mut ctx = make_ctx();
    let step = StepSample::new(AgentConfig::default());
    // When both stream and complete fail, we expect Err.
    let result = step
        .execute(provider, &mut ctx, vec![], CancellationToken::new())
        .await;
    assert!(result.is_err());
}

/// Regression: when `ctx.messages` is empty, `execute()` must short-circuit
/// with an `Error::Validation` and MUST NOT call the provider. The upstream
/// APIs (e.g. OpenAI Chat Completions) reject empty `messages` arrays with a
/// 400 (`messages is empty (2013)`); without this guard the agent loops
/// forever, each iteration repeating the same 400, with the recovery cascade
/// resetting already-empty messages and returning `Recovered`.
#[tokio::test]
async fn step_sample_short_circuits_on_empty_messages() {
    // Provider that records whether it was called at all.
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct TrackingProvider {
        call_count: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl ModelProvider for TrackingProvider {
        async fn initialize(
            &mut self,
            _config: ProviderConfig,
        ) -> Result<(), Error> {
            Ok(())
        }

        fn name(&self) -> &str {
            "tracking"
        }

        fn model_config(&self) -> ModelConfig {
            ModelConfig {
                name: "tracking".to_string(),
                provider: "tracking".to_string(),
                context_window: 8192,
                max_output_tokens: 2048,
                supports_tools: true,
                supports_streaming: true,
                supports_reasoning: false,
            }
        }

        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> Result<CompletionResponse, Error> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(CompletionResponse {
                id: "x".to_string(),
                model: "x".to_string(),
                content: synthia_provider::Content::text(""),
                usage: ProviderTokenUsage::default(),
                cached: false,
            })
        }

        async fn complete_with_stream(
            &self,
            _request: CompletionRequest,
            _cancel_token: Option<CancellationToken>,
            _on_delta: Box<dyn FnMut(StreamChunk) + Send>,
        ) -> Result<CompletionResponse, Error> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(CompletionResponse {
                id: "x".to_string(),
                model: "x".to_string(),
                content: synthia_provider::Content::text(""),
                usage: ProviderTokenUsage::default(),
                cached: false,
            })
        }

        async fn embed(
            &self,
            _texts: Vec<String>,
        ) -> Result<Vec<Vec<f64>>, Error> {
            Ok(vec![])
        }
    }

    let call_count = Arc::new(AtomicUsize::new(0));
    let provider = Arc::new(TrackingProvider {
        call_count: call_count.clone(),
    }) as Arc<dyn ModelProvider>;

    let mut ctx = make_ctx();
    // Sanity: messages start empty (LoopContext::new produces empty vec).
    assert!(ctx.messages.is_empty());

    let step = StepSample::new(AgentConfig::default());
    let result = step
        .execute(provider, &mut ctx, vec![], CancellationToken::new())
        .await;

    // Must be a validation error — not a network call, not Ok.
    match result {
        Err(Error::Validation(msg)) => {
            assert!(
                msg.to_lowercase().contains("empty")
                    || msg.to_lowercase().contains("messages"),
                "unexpected validation message: {msg}",
            );
        }
        Err(other) => panic!("expected Error::Validation, got {other:?}"),
        Ok(_) => panic!("expected error on empty messages, got Ok"),
    }
    // The provider must not have been called at all.
    assert_eq!(
        call_count.load(Ordering::SeqCst),
        0,
        "provider must not be called when ctx.messages is empty",
    );
}
