//! Shared fixtures for the [`super`] test suite.
//!
//! Every test submodule pulls from this file; the per-section
//! helpers (`tools_with`, `build_panel_fixture`,
//! `run_and_collect_with_restriction`, …) live with the test
//! that uses them, not here. Only the cross-section fixtures
//! live in one place so the test bodies stay byte-identical
//! to the previous monolithic `tests.rs`.
//!
//! [`super`]: crate::agent::re_act

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use futures::StreamExt;
use synthia_core::{CancelToken, Error};
use synthia_provider::{
    CompletionResponse,
    ContentPart,
    ProviderConfig,
    SamplingResult,
    StreamChunk,
    TokenUsage,
    ToolDefinition,
    ToolUse,
    traits::ModelProvider,
    types::ModelConfig,
};
use synthia_tool::ToolRegistry;
use tokio::sync::Mutex as TokioMutex;
use tokio_util::sync::CancellationToken;

use super::*;

// -- ScriptedStreamProvider -------------------------------------------

/// Test provider that emits a pre-scripted sequence of
/// `StreamChunk`s per `complete_with_stream` call.
#[derive(Debug)]
pub(super) struct ScriptedStreamProvider {
    scripted: Arc<TokioMutex<Vec<Vec<StreamChunk>>>>,
    call_count: Arc<AtomicUsize>,
}

impl ScriptedStreamProvider {
    pub(super) fn new(scripted: Vec<Vec<StreamChunk>>) -> Self {
        Self {
            scripted: Arc::new(TokioMutex::new(scripted)),
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    async fn take_next(&self) -> Vec<StreamChunk> {
        let mut guard = self.scripted.lock().await;
        if guard.is_empty() {
            return vec![StreamChunk::IsDone {
                result: Box::new(SamplingResult::default()),
            }];
        }
        guard.remove(0)
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
            name: "fake".to_string(),
            provider: "scripted".to_string(),
            context_window: 128_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
        }
    }

    async fn complete(
        &self,
        _request: synthia_provider::CompletionRequest,
    ) -> Result<CompletionResponse, Error> {
        unreachable!("streaming path should not call complete()")
    }

    async fn complete_with_stream(
        &self,
        _request: synthia_provider::CompletionRequest,
        _cancel_token: Option<Arc<dyn CancelToken>>,
        mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> Result<CompletionResponse, Error> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let chunks = self.take_next().await;
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
            model: "fake".to_string(),
            content: synthia_provider::Content::Single(ContentPart::Text(
                TextContent {
                    text: sampling.text.clone(),
                    cache_control: None,
                },
            )),
            usage: sampling.usage.clone(),
            cached: false,
            replay_state: None,
            stop_reason: sampling.stop_reason.clone(),
        })
    }
}

// -- CapturingProvider -----------------------------------------------

/// Provider that records every `CompletionRequest` it receives.
/// Used to assert what the ReAct loop actually sends to the
/// LLM, not just what is stored on the agent.
pub(super) struct CapturingProvider {
    pub(super) captured:
        Arc<TokioMutex<Vec<Arc<Vec<synthia_provider::Message>>>>>,
    pub(super) captured_tools: Arc<TokioMutex<Vec<Arc<Vec<ToolDefinition>>>>>,
    scripted: Arc<TokioMutex<Vec<Vec<StreamChunk>>>>,
    pub(super) call_count: AtomicUsize,
}

impl CapturingProvider {
    pub(super) fn new(scripted: Vec<Vec<StreamChunk>>) -> Self {
        Self {
            captured: Arc::new(TokioMutex::new(Vec::new())),
            captured_tools: Arc::new(TokioMutex::new(Vec::new())),
            scripted: Arc::new(TokioMutex::new(scripted)),
            call_count: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl ModelProvider for CapturingProvider {
    async fn initialize(
        &mut self,
        _config: ProviderConfig,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn name(&self) -> &str {
        "capturing"
    }

    fn model_config(&self) -> ModelConfig {
        ModelConfig {
            name: "capturing".into(),
            provider: "test".into(),
            context_window: 128_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        }
    }

    async fn complete(
        &self,
        _request: synthia_provider::types::CompletionRequest,
    ) -> Result<CompletionResponse, Error> {
        unreachable!("streaming path")
    }

    async fn complete_with_stream(
        &self,
        request: synthia_provider::types::CompletionRequest,
        _cancel: Option<Arc<dyn CancelToken>>,
        mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> Result<CompletionResponse, Error> {
        self.captured.lock().await.push(request.messages.clone());
        self.captured_tools.lock().await.push(request.tools.clone());
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let chunks = {
            let mut g = self.scripted.lock().await;
            if g.is_empty() {
                vec![StreamChunk::IsDone {
                    result: Box::new(SamplingResult::default()),
                }]
            } else {
                g.remove(0)
            }
        };
        let mut sampling: Option<SamplingResult> = None;
        for c in chunks {
            if let StreamChunk::IsDone { result } = &c {
                sampling = Some((**result).clone());
            }
            on_delta(c);
        }
        let s = sampling.unwrap_or_default();
        Ok(CompletionResponse {
            id: "cap".into(),
            model: "capturing".into(),
            content: synthia_provider::Content::Single(ContentPart::Text(
                synthia_provider::TextContent {
                    text: s.text.clone(),
                    cache_control: None,
                },
            )),
            usage: s.usage.clone(),
            cached: false,
            replay_state: None,
            stop_reason: s.stop_reason.clone(),
        })
    }
}

// -- Chunk-sequence helpers ------------------------------------------

pub(super) fn empty_response() -> Vec<StreamChunk> {
    vec![StreamChunk::IsDone {
        result: Box::new(SamplingResult {
            text: "hi there".to_string(),
            tool_calls: vec![],
            reasoning: String::new(),
            reasoning_signature: None,
            usage: TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
                cached_prompt_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
                reasoning_tokens: None,
            },
            ..Default::default()
        }),
    }]
}

pub(super) fn tool_call_response(tool_calls: Vec<ToolUse>) -> Vec<StreamChunk> {
    let mut chunks = Vec::new();
    for call in &tool_calls {
        chunks.push(StreamChunk::ToolCallStart {
            id: call.id.clone(),
            name: call.name.clone(),
            arguments: serde_json::Value::String(
                serde_json::to_string(&call.input).unwrap_or_default(),
            ),
        });
        chunks.push(StreamChunk::ToolCallEnd {
            id: call.id.clone(),
        });
    }
    // Real providers re-list tool calls in `IsDone`; the loop
    // consumes them only once (via `ToolCallEnd`), so emit empty
    // here to avoid double-counting.
    chunks.push(StreamChunk::IsDone {
        result: Box::new(SamplingResult {
            text: String::new(),
            tool_calls: Vec::new(),
            reasoning: String::new(),
            reasoning_signature: None,
            usage: TokenUsage::default(),
            ..Default::default()
        }),
    });
    chunks
}

/// `StreamChunk::Usage` carrying a non-default
/// [`TokenUsage`]. Used by tests that need to assert the
/// per-iteration Usage emission pipeline.
pub(super) fn tool_call_response_with_usage(
    tool_calls: Vec<ToolUse>,
    usage: TokenUsage,
) -> Vec<StreamChunk> {
    let mut chunks = vec![StreamChunk::Usage(usage)];
    chunks.extend(tool_call_response(tool_calls));
    chunks
}

// -- Run helper ------------------------------------------------------

// -- Echo registry helper --------------------------------------------

/// Registry with one `echo` tool; returns the execution counter.
pub(super) fn echo_registry()
-> (Arc<ToolRegistry>, Arc<parking_lot::Mutex<usize>>) {
    let tool = synthia_test_support::FakeTool::new("echo", "echoed");
    let executions = Arc::clone(&tool.call_count);
    let registry = Arc::new(ToolRegistry::new());
    registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(tool)));
    (registry, executions)
}

/// Run one [`ReActAgent`] session and drain every [`AgentEvent`]
/// through the channel. Returns the captured event sequence.
pub(super) async fn run_and_collect(
    provider: Arc<dyn ModelProvider>,
    registry: Arc<ToolRegistry>,
    cancel: CancellationToken,
    input: AgentInput,
) -> Vec<AgentEvent> {
    let agent = ReActAgent::new(provider, registry);
    let mut stream = agent.run(input, Arc::new(cancel)).await;
    let mut out = Vec::new();
    while let Some(ev) = stream.next().await {
        out.push(ev);
    }
    out
}

// -- Peer-agent descriptor (shared by prompt integration + panel) ----

/// Build a peer [`AgentDescriptor`] for the panel fixture.
/// Only `name`, `description`, and `handoff_hint` are
/// exercised by the prompt assembler, so every other
/// field is filled with a minimal sentinel.
pub(super) fn peer_descriptor(
    name: &str,
    description: &str,
    handoff_hint: Option<&str>,
) -> AgentDescriptor {
    AgentDescriptor {
        name: name.into(),
        description: description.into(),
        kind: "panel".into(),
        version: "1.0.0".into(),
        instructions: "".into(),
        capabilities: Vec::new(),
        tools: Vec::new(),
        model_hint: None,
        handoffs: Vec::new(),
        handoff_hint: handoff_hint.map(str::to_string),
        output_schema: None,
        owner: None,
        domain: None,
        persona: None,
        display_name: None,
        max_iterations: None,
    }
}
