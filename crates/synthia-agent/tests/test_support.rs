#![allow(deprecated)]
//! Test support utilities for synthia-agent integration tests.
//!
//! Provides FakeProvider and FakeTool implementations for testing
//! the agent loop without requiring real LLM API calls.
//!
//! Method-level `#[allow(dead_code)]` is used per-builder method because
//! this is shared test infrastructure: each integration test exercises
//! a subset of the builders, and per-test compilation emits dead-code
//! warnings for the unused ones. The same pattern is used in
//! `tests/explicit_recovery_paths_test.rs:315`.

#![allow(dead_code)]

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use synthia_agent::{
    AgentResult,
    AgentStatus,
    AgentTokenUsage,
    config::{AgentConfig, AgentRunConfig, AgentRunConfigBuilder},
    steering::SteeringChannel,
    subagent::{
        ChildSessionHandle,
        SubagentSessionError,
        SubagentSessionFactory,
    },
    types::AgentInput,
};
use synthia_context::ContextAssembler;
use synthia_core::Error;
use synthia_hook::HookRegistry;
use synthia_provider::{router::ModelRouter, traits::ModelProvider, types::*};
use synthia_session::{Store as SessionStore, store::SessionInputQueue};
use synthia_tool::{traits::Tool, types::ToolOutput};

/// A fake tool that returns a predetermined response.
pub struct FakeTool {
    name: String,
    output: String,
    should_fail: bool,
    error_message: String,
    /// When `true`, `requires_permission()` returns `true` so the
    /// registry invokes the permission checker for this tool. This is
    /// required to exercise the cascade's `Err(e)` arm in
    /// `stream_builder/builder.rs` (the only registry-level error
    /// path goes through the permission checker). Defaults to `false`
    /// for backward compatibility with existing tests.
    requires_permission: bool,
}

impl FakeTool {
    pub fn new(name: &str, output: &str) -> Self {
        Self {
            name: name.to_string(),
            output: output.to_string(),
            should_fail: false,
            error_message: String::new(),
            requires_permission: false,
        }
    }

    pub fn failing(name: &str, error_message: &str) -> Self {
        Self {
            name: name.to_string(),
            output: String::new(),
            should_fail: true,
            error_message: error_message.to_string(),
            requires_permission: false,
        }
    }

    /// Make the tool require permission. Use with
    /// `PermissionChecker::always_fail_for_test` to force
    /// `StepToolExecute::execute` to return `Err` so the L3-L5
    /// recovery cascade's `Err` arm in `stream_builder/builder.rs`
    /// can be exercised end-to-end.
    pub fn with_requires_permission(mut self) -> Self {
        self.requires_permission = true;
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

    fn requires_permission(&self) -> bool {
        self.requires_permission
    }

    async fn call(&self, _input: synthia_tool::types::ToolInput) -> ToolOutput {
        if self.should_fail {
            ToolOutput::error(&self.error_message)
        } else {
            ToolOutput::text(&self.output)
        }
    }
}

/// A fake LLM provider that returns predetermined responses.
pub struct FakeProvider {
    responses: Vec<String>,
    stream_chunks: Vec<Vec<StreamChunk>>,
    /// Per-call scripted errors. When `completion_errors[idx]` is `Some`
    /// the corresponding call to `complete_with_stream` returns that
    /// error string as `Err`. When the index is out of range, the call
    /// succeeds with the canned chunks.
    completion_errors: Vec<Option<String>>,
    /// Shared counter between `complete()` and `complete_with_stream()`.
    ///
    /// Some existing tests (e.g. `explicit_recovery_paths_test.rs`)
    /// script errors by assuming each logical LLM call consumes two
    /// slots: one for `complete_with_stream` and one for the synchronous
    /// `complete()` fallback. The default shared counter preserves that
    /// behaviour.
    call_count: Arc<Mutex<usize>>,
    /// Optional separate counter for `complete()` calls.
    ///
    /// Tests where a tool internally calls `complete()` (e.g.
    /// `SelfReflectTool::run_self_reflect`) can opt into separate
    /// counters so the tool's `complete()` call does not shift the
    /// indices used by `complete_with_stream()`.
    complete_count: Option<Arc<Mutex<usize>>>,
}

impl FakeProvider {
    pub fn new(responses: Vec<String>) -> Self {
        Self {
            responses,
            stream_chunks: Vec::new(),
            completion_errors: Vec::new(),
            call_count: Arc::new(Mutex::new(0)),
            complete_count: None,
        }
    }

    /// Use a separate counter for `complete()` calls so tool-internal
    /// LLM calls do not shift `complete_with_stream()` indices.
    pub fn with_separate_complete_counter(mut self) -> Self {
        self.complete_count = Some(Arc::new(Mutex::new(0)));
        self
    }

    pub fn with_stream_chunks(mut self, chunks: Vec<Vec<StreamChunk>>) -> Self {
        self.stream_chunks = chunks;
        self
    }

    /// Make the n-th call (0-indexed) to `complete_with_stream` return
    /// `Err(error)`. Useful for scripting LLM sampling failures so the
    /// recovery cascade path can be exercised end-to-end.
    pub fn with_completion_errors(
        mut self,
        errors: Vec<Option<String>>,
    ) -> Self {
        self.completion_errors = errors;
        self
    }

    pub fn with_response(mut self, response: &str) -> Self {
        self.stream_chunks = vec![vec![
            StreamChunk::Content(ContentPart::Text(TextContent {
                text: response.to_string(),
                cache_control: None,
            })),
            StreamChunk::Stop("end_turn".into()),
        ]];
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
        let idx = if let Some(ref c) = self.complete_count {
            let mut count = c.lock().unwrap();
            let idx = *count;
            *count += 1;
            idx
        } else {
            let mut count = self.call_count.lock().unwrap();
            let idx = *count;
            *count += 1;
            idx
        };
        eprintln!(
            "FAKE complete idx={} responses_len={}",
            idx,
            self.responses.len()
        );

        if let Some(Some(err)) = self.completion_errors.get(idx) {
            return Err(synthia_core::Error::Provider(format!(
                "synthetic completion error: {err}"
            )));
        }

        let text = if idx < self.responses.len() {
            self.responses[idx].clone()
        } else {
            self.responses.last().cloned().unwrap_or_default()
        };
        eprintln!("FAKE complete idx={} returning: {}", idx, text);

        Ok(CompletionResponse {
            id: "fake".to_string(),
            model: "fake-model".to_string(),
            content: Content::text(text),
            usage: Default::default(),
            cached: false,
        })
    }

    async fn complete_with_stream(
        &self,
        _request: CompletionRequest,
        _cancel_token: Option<tokio_util::sync::CancellationToken>,
        mut on_delta: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> Result<CompletionResponse, Error> {
        let idx = {
            let mut count = self.call_count.lock().unwrap();
            let idx = *count;
            *count += 1;
            idx
        };
        eprintln!(
            "FAKE complete_with_stream idx={} chunks_len={}",
            idx,
            self.stream_chunks.len()
        );

        if let Some(Some(err)) = self.completion_errors.get(idx) {
            return Err(synthia_core::Error::Provider(format!(
                "synthetic completion error: {err}"
            )));
        }

        let chunks = if idx < self.stream_chunks.len() {
            self.stream_chunks[idx].clone()
        } else if !self.stream_chunks.is_empty() {
            self.stream_chunks.last().cloned().unwrap_or_default()
        } else {
            vec![StreamChunk::Stop("end_turn".into())]
        };

        for chunk in &chunks {
            on_delta(chunk.clone());
        }

        // Replay the legacy logic: text chunks accumulate into `text`,
        // tool_use chunks accumulate into `tool_calls`, then a Stop ends
        // the stream. We fold the chunks into a SamplingResult and emit
        // it as IsDone so the agent loop has authoritative end-of-stream
        // information.
        let mut text = String::new();
        let mut tool_calls = Vec::new();
        for chunk in &chunks {
            match chunk {
                StreamChunk::Content(ContentPart::Text(t)) => {
                    text.push_str(&t.text);
                }
                StreamChunk::Content(ContentPart::ToolUse(tu)) => {
                    tool_calls.push(tu.clone());
                }
                _ => {}
            }
        }
        on_delta(StreamChunk::IsDone {
            result: Box::new(SamplingResult {
                text,
                tool_calls,
                reasoning: String::new(),
                usage: TokenUsage::default(),
            }),
        });

        Ok(CompletionResponse {
            id: "fake".to_string(),
            model: "fake-model".to_string(),
            content: Content::text(""),
            usage: TokenUsage::default(),
            cached: false,
        })
    }

    async fn embed(
        &self,
        _texts: Vec<String>,
    ) -> Result<Vec<Vec<f64>>, synthia_core::Error> {
        Ok(vec![vec![0.0; 1536]; _texts.len()])
    }
}

/// A fake [`SubagentSessionFactory`] that returns a deterministic result
/// without creating real child sessions. Useful for exercising
/// sub-agent tool calls in integration tests without starting a server.
pub struct FakeSubagentFactory {
    output: String,
}

impl FakeSubagentFactory {
    pub fn new(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
        }
    }
}

impl Default for FakeSubagentFactory {
    fn default() -> Self {
        Self::new("the answer 42")
    }
}

#[async_trait]
impl SubagentSessionFactory for FakeSubagentFactory {
    async fn create_child(
        &self,
        _user_id: String,
        _parent_session_id: String,
        _maybe_id: Option<String>,
        _parent_depth: usize,
    ) -> Result<ChildSessionHandle, SubagentSessionError> {
        Err(SubagentSessionError::CreationFailed(
            "FakeSubagentFactory does not create real sessions".to_string(),
        ))
    }

    async fn run_child(
        &self,
        _user_id: String,
        _parent_session_id: String,
        prompt: String,
        _parent_depth: usize,
        _maybe_id: Option<String>,
    ) -> Result<AgentResult, SubagentSessionError> {
        Ok(AgentResult {
            output: format!("{}\n\n{}", prompt, self.output),
            status: AgentStatus::Completed,
            token_usage: AgentTokenUsage {
                input_tokens: 0,
                output_tokens: 0,
            },
        })
    }
}

/// Helper function to create a test workspace directory.
pub fn create_test_workspace() -> PathBuf {
    let temp_dir = tempfile::tempdir().unwrap();
    temp_dir.path().to_path_buf()
}

// ---------------------------------------------------------------------------
// AgentRunConfig builders
// ---------------------------------------------------------------------------
//
// Centralized scaffolding for integration tests. The previous version of
// each test file held its own ~25-line `make_run_config` copy; this module
// collapses them into three public helpers:
//
//   * `make_run_config` — base 7-arg builder, 4096-token context.
//   * `make_run_config_with_steering` — adds a `steering_channel`.
//   * `make_run_config_with_assembler` — caller-supplied `context_assembler`.
//
// `Arc<FakeProvider>` coerces to `Arc<dyn ModelProvider>`, so callers can
// pass their `Arc<FakeProvider>` directly. The model router and session
// store are constructed internally with sensible defaults; override them
// by using `AgentRunConfigBuilder` directly when a test needs full control.

/// Test user identifier used by all integration tests in this crate.
/// Centralised so the per-test scaffolding stays consistent with the
/// non-empty `user_id` requirement enforced by `AgentRunConfigBuilder`.
pub const TEST_USER_ID: &str = "test-user";

/// Build a base `AgentRunConfig` with a 4096-token `ContextAssembler` and
/// the default `ModelRouter`. The `provider` is the only piece of
/// non-default test infrastructure callers must supply.
pub fn make_run_config(
    provider: Arc<dyn ModelProvider>,
    tool_registry: synthia_tool::registry::ToolRegistry,
    hook_registry: HookRegistry,
    session_id: String,
    input: AgentInput,
    config: AgentConfig,
    cancel_token: tokio_util::sync::CancellationToken,
) -> AgentRunConfig {
    make_run_config_with_assembler(
        provider,
        tool_registry,
        hook_registry,
        TEST_USER_ID.to_string(),
        session_id,
        input,
        config,
        Arc::new(ContextAssembler::new(4096)),
        cancel_token,
    )
}

/// Same as `make_run_config` but attaches a `steering_channel` for tests
/// that drive `AgentEvent::SteeringReceived`. Messages already sent to the
/// `steering_channel` are drained and pushed into a `SessionInputQueue`
/// so the main loop picks them up.
#[allow(clippy::too_many_arguments)]
pub fn make_run_config_with_steering(
    provider: Arc<dyn ModelProvider>,
    tool_registry: synthia_tool::registry::ToolRegistry,
    hook_registry: HookRegistry,
    user_id: String,
    session_id: String,
    input: AgentInput,
    config: AgentConfig,
    context_assembler: Arc<ContextAssembler>,
    cancel_token: tokio_util::sync::CancellationToken,
    steering_channel: Arc<dyn SteeringChannel>,
) -> AgentRunConfig {
    let sessions_root = config.workspace_root.join(".synthia").join("sessions");
    let session_store = SessionStore::new(sessions_root.clone());
    let input_queue = SessionInputQueue::new(sessions_root);

    // Drain any steering messages already queued into the persistent queue
    while let Some(msg) = steering_channel.try_recv() {
        let _ = input_queue.push(&user_id, &session_id, msg.content, 0u8);
    }

    AgentRunConfigBuilder::new()
        .provider(provider)
        .tool_registry(tool_registry)
        .hook_registry(Arc::new(hook_registry))
        .model_router(Arc::new(ModelRouter::new()))
        .user_id(user_id)
        .session_id(session_id)
        .input(input)
        .config(config)
        .context_assembler(context_assembler)
        .session_store(session_store)
        .session_input_queue(input_queue)
        .cancel_token(cancel_token)
        .build()
        .unwrap()
}

/// Same as `make_run_config` but with a caller-supplied `context_assembler`.
/// Use when a test depends on a specific context size (e.g. memory tests
/// need 8192 tokens to fit prior turns). Most tests should prefer the
/// default `make_run_config` (4096 tokens) for consistency.
#[allow(clippy::too_many_arguments)]
pub fn make_run_config_with_assembler(
    provider: Arc<dyn ModelProvider>,
    tool_registry: synthia_tool::registry::ToolRegistry,
    hook_registry: HookRegistry,
    user_id: String,
    session_id: String,
    input: AgentInput,
    config: AgentConfig,
    context_assembler: Arc<ContextAssembler>,
    cancel_token: tokio_util::sync::CancellationToken,
) -> AgentRunConfig {
    let session_store = SessionStore::new(
        config.workspace_root.join(".synthia").join("sessions"),
    );
    AgentRunConfigBuilder::new()
        .provider(provider)
        .tool_registry(tool_registry)
        .hook_registry(Arc::new(hook_registry))
        .model_router(Arc::new(ModelRouter::new()))
        .user_id(user_id)
        .session_id(session_id)
        .input(input)
        .config(config)
        .context_assembler(context_assembler)
        .session_store(session_store)
        .cancel_token(cancel_token)
        .build()
        .unwrap()
}
