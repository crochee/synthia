//! Test support utilities for synthia-agent integration tests.
//!
//! Provides FakeProvider and FakeTool implementations for testing
//! the agent loop without requiring real LLM API calls.
//!
//! Each integration test binary in this crate includes this file
//! via `mod test_support`, so every symbol must compile under every
//! binary. Individual symbols are still only used by the binaries
//! that exercise them, hence the file-level `dead_code` allowance.

// Shared infrastructure for every integration test binary — each
// binary compiles this file independently and uses a subset.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use synthia_agent::AgentInput;
use synthia_core::Error;
use synthia_provider::{
    SamplingResult,
    StreamChunk,
    TokenUsage,
    traits::ModelProvider,
    types::{
        CompletionRequest,
        CompletionResponse,
        Content,
        ContentPart,
        ModelConfig,
        ProviderConfig,
        TextContent,
    },
};
use synthia_tool::{
    traits::Tool,
    types::{Context, ToolOutput, TruncatedBy},
};
use tokio_util::sync::CancellationToken;

/// A fake tool that returns a predetermined response.
pub struct FakeTool {
    name: String,
    output: String,
    should_fail: bool,
    error_message: String,
    /// When `true`, the tool advertises
    /// [`synthia_tool::traits::ExecutionMode::Sequential`].
    /// Defaults to `false` (parallel, matches the trait
    /// default).
    sequential: bool,
    /// Optional metadata entries the tool attaches to its
    /// `ToolOutput::metadata`. Tests use this to verify the
    /// agent loop forwards them onto the wire `ToolResult`.
    metadata: Vec<(String, serde_json::Value)>,
    /// Optional truncation reason the tool attaches to
    /// its `ToolOutput::truncated_by`. Tests use this to
    /// verify the agent loop serialises it onto the wire
    /// without dropping the information.
    truncated_by: Option<TruncatedBy>,
}

impl FakeTool {
    pub fn new(name: &str, output: &str) -> Self {
        Self {
            name: name.to_string(),
            output: output.to_string(),
            should_fail: false,
            error_message: String::new(),
            sequential: false,
            metadata: Vec::new(),
            truncated_by: None,
        }
    }

    pub fn failing(name: &str, error_message: &str) -> Self {
        Self {
            name: name.to_string(),
            output: String::new(),
            should_fail: true,
            error_message: error_message.to_string(),
            sequential: false,
            metadata: Vec::new(),
            truncated_by: None,
        }
    }

    /// Tag the tool as [`synthia_tool::traits::ExecutionMode::Sequential`]
    /// so it runs strictly in the order the LLM requested it and
    /// aborts the round on the first `is_error` result.
    pub fn sequential(mut self) -> Self {
        self.sequential = true;
        self
    }

    /// Attach a metadata entry to the tool's
    /// `ToolOutput::metadata`. Builder-style; can be chained.
    pub fn with_metadata(
        mut self,
        key: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        self.metadata.push((key.into(), value));
        self
    }

    /// Attach a truncation reason to the tool's
    /// `ToolOutput::truncated_by`. Builder-style.
    pub fn with_truncated_by(mut self, t: TruncatedBy) -> Self {
        self.truncated_by = Some(t);
        self
    }
}

#[async_trait]
impl Tool for FakeTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "A fake tool for testing"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn mode(&self) -> synthia_tool::traits::ExecutionMode {
        if self.sequential {
            synthia_tool::traits::ExecutionMode::Sequential
        } else {
            synthia_tool::traits::ExecutionMode::Parallel
        }
    }

    async fn call(
        &self,
        _input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        let mut out = if self.should_fail {
            ToolOutput::error(&self.error_message)
        } else {
            ToolOutput::text(&self.output)
        };
        for (k, v) in &self.metadata {
            out = out.with_metadata(k.clone(), v.clone());
        }
        if let Some(t) = &self.truncated_by {
            out = out.with_truncated_by(t.clone());
        }
        out
    }
}

/// A fake LLM provider that returns predetermined responses.
pub struct FakeProvider {
    responses: Vec<Content>,
    call_count: Arc<Mutex<usize>>,
}

impl FakeProvider {
    /// Build a provider whose first N calls return each of `responses`
    /// as plain text. Calls beyond N repeat the last response.
    pub fn new(responses: Vec<String>) -> Self {
        Self {
            responses: responses.into_iter().map(Content::text).collect(),
            call_count: Arc::new(Mutex::new(0)),
        }
    }

    /// Build a provider whose first N calls return each of `responses`
    /// verbatim, preserving tool-call structure.
    pub fn new_content(responses: Vec<Content>) -> Self {
        Self {
            responses,
            call_count: Arc::new(Mutex::new(0)),
        }
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
        let idx = {
            let mut count = self.call_count.lock().unwrap();
            let idx = *count;
            *count += 1;
            idx
        };

        let content = if idx < self.responses.len() {
            self.responses[idx].clone()
        } else {
            self.responses
                .last()
                .cloned()
                .unwrap_or_else(|| synthia_provider::Content::text(""))
        };

        Ok(CompletionResponse {
            id: "fake".to_string(),
            model: "fake-model".to_string(),
            content,
            usage: Default::default(),
            cached: false,
            stop_reason: None,
        })
    }

    async fn embed(
        &self,
        _texts: Vec<String>,
    ) -> Result<Vec<Vec<f64>>, synthia_core::Error> {
        Ok(vec![vec![0.0; 1536]; _texts.len()])
    }
}

/// Build a [`synthia_agent::ReActAgent`] from the same fixtures as
/// `make_run_config`. Convenience wrapper so the migrated
/// `mvp_run_test.rs` can drop straight into the new Stream API.
pub fn make_react_agent(
    provider: Arc<dyn ModelProvider>,
    tool_registry: Arc<synthia_tool::registry::ToolRegistry>,
    session_id: String,
    input: AgentInput,
    cancel_token: CancellationToken,
    user_id: Option<String>,
) -> (
    std::sync::Arc<synthia_agent::ReActAgent>,
    AgentInput,
    std::sync::Arc<tokio_util::sync::CancellationToken>,
) {
    let _ = (session_id, user_id); // preserved for compatibility
    let agent = std::sync::Arc::new(synthia_agent::ReActAgent::new(
        provider,
        tool_registry,
    ));
    (agent, input, std::sync::Arc::new(cancel_token))
}

// ---------------------------------------------------------------------------
// ScriptedStreamProvider — streaming fake for integration tests
// ---------------------------------------------------------------------------

/// Streaming fake provider. Each `complete_with_stream` call pops one
/// scripted `Vec<StreamChunk>` from the queue and emits every chunk
/// through the provider's `on_delta` callback exactly the way a real
/// streaming backend would.
pub struct ScriptedStreamProvider {
    scripted: Arc<tokio::sync::Mutex<Vec<Vec<StreamChunk>>>>,
    call_count: Arc<std::sync::atomic::AtomicUsize>,
}

impl ScriptedStreamProvider {
    pub fn new(scripted: Vec<Vec<StreamChunk>>) -> Self {
        Self {
            scripted: Arc::new(tokio::sync::Mutex::new(scripted)),
            call_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Build from a sequence of `Content` responses, emitting each as a
    /// single `IsDone { result }` chunk (the path real providers take
    /// when they buffer the full response).
    pub fn from_content_responses(responses: Vec<Content>) -> Self {
        let chunks: Vec<Vec<StreamChunk>> = responses
            .into_iter()
            .map(|c| {
                // Normalize to a flat list of parts (handles
                // `Content::Single(part)` vs `Content::Multi(parts)`).
                let mut text = String::new();
                let mut calls = Vec::new();
                for p in c {
                    match p {
                        ContentPart::Text(t) => text.push_str(&t.text),
                        ContentPart::ToolUse(tu) => calls.push(tu),
                        _ => {}
                    }
                }
                vec![StreamChunk::IsDone {
                    result: Box::new(SamplingResult {
                        text,
                        tool_calls: calls,
                        reasoning: String::new(),
                        reasoning_signature: None,
                        usage: TokenUsage::default(),
                        ..Default::default()
                    }),
                }]
            })
            .collect();
        Self::new(chunks)
    }
}

#[async_trait]
impl ModelProvider for ScriptedStreamProvider {
    async fn initialize(
        &mut self,
        _config: ProviderConfig,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn name(&self) -> &str {
        "scripted-stream"
    }

    fn model_config(&self) -> ModelConfig {
        ModelConfig {
            name: "fake-stream".to_string(),
            provider: "fake".to_string(),
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
        unreachable!("streaming path should not call complete()")
    }

    async fn complete_with_stream(
        &self,
        _request: CompletionRequest,
        _cancel_token: Option<CancellationToken>,
        mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> Result<CompletionResponse, Error> {
        use std::sync::atomic::Ordering;
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let chunks = {
            let mut guard = self.scripted.lock().await;
            if guard.is_empty() {
                vec![StreamChunk::IsDone {
                    result: Box::new(SamplingResult::default()),
                }]
            } else {
                guard.remove(0)
            }
        };
        let mut final_sampling: Option<SamplingResult> = None;
        for chunk in chunks {
            if let StreamChunk::IsDone { result } = &chunk {
                final_sampling = Some((**result).clone());
            }
            on_delta(chunk);
        }
        let sampling = final_sampling.unwrap_or_default();
        Ok(CompletionResponse {
            id: format!("resp-{}", self.call_count.load(Ordering::SeqCst)),
            model: "fake-stream".to_string(),
            content: Content::Single(ContentPart::Text(TextContent {
                text: sampling.text.clone(),
                cache_control: None,
            })),
            usage: sampling.usage.clone(),
            cached: false,
            stop_reason: sampling.stop_reason.clone(),
        })
    }

    async fn embed(&self, _texts: Vec<String>) -> Result<Vec<Vec<f64>>, Error> {
        Ok(vec![])
    }
}
