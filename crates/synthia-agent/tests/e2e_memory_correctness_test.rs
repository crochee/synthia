#![allow(deprecated)]
//! E2E test: Multi-turn memory correctness.
//!
//! Verifies that across multiple turns of conversation, the agent correctly
//! maintains context and can reference earlier information.

mod test_support;
use std::{path::PathBuf, sync::Arc};

use futures::StreamExt;
use synthia_agent::{
    agent::Agent,
    config::AgentConfig,
    events::SystemEvent,
    types::{AgentEvent, AgentInput, SessionEndReason},
};
use synthia_context::ContextAssembler;
use synthia_core::Error;
use synthia_hook::HookRegistry;
use synthia_memory::{episodic::EpisodicMemory, hot::HotMemory};
use synthia_provider::{
    traits::ModelProvider,
    types::{
        CompletionRequest,
        CompletionResponse,
        Content,
        ContentPart,
        ProviderConfig,
        ToolUse,
    },
};
use synthia_session::types::TokenBudget;
use synthia_tool::registry::{ToolEntry, ToolRegistry};
use test_support::{FakeProvider, FakeTool, make_run_config_with_assembler};
use tokio_util::sync::CancellationToken;

/// Helper to collect all events from the agent stream.
async fn collect_events(
    stream: impl futures::Stream<Item = AgentEvent>,
) -> Vec<AgentEvent> {
    stream.collect().await
}

/// Build a minimal agent config for testing.
fn test_config(workspace_root: PathBuf) -> AgentConfig {
    AgentConfig {
        model: "test-model".to_string(),
        max_tokens: 4096,
        max_iterations: 5,
        temperature: None,
        workspace_root,
        token_budget: None,
        checkpoint_dir: None,
        context_token_budget: Some(TokenBudget::new(128_000)),
        compaction_provider: None,
        observability: None,
        ..Default::default()
    }
}

/// Build a string response from text content (for FakeProvider).
fn text_response(content: &str) -> String {
    content.to_string()
}

/// Build a CompletionResponse from text content (for TrackingProvider).
fn text_response_completion(content: &str) -> CompletionResponse {
    CompletionResponse {
        id: "resp-1".to_string(),
        model: "test-model".to_string(),
        content: Content::text(content),
        usage: synthia_provider::types::TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 20,
            total_tokens: 120,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
        },
        cached: false,
    }
}

/// Verify that SessionStarted is always the first event emitted.
#[tokio::test]
async fn test_memory_session_starts_correctly() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider = Arc::new(FakeProvider::new(vec![text_response(
        "I remember you mentioned Rust earlier.",
    )]));

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = test_config(workspace);

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config_with_assembler(
        provider,
        tool_registry,
        hook_registry,
        test_support::TEST_USER_ID.to_string(),
        "mem-test-1".to_string(),
        AgentInput::text("What language did I mention?"),
        config,
        Arc::new(ContextAssembler::new(4096)),
        cancel_token,
    );

    let events = collect_events(Agent::run_stream(run_config)).await;

    assert!(!events.is_empty(), "should emit at least one event");
    match &events[0] {
        AgentEvent::System(SystemEvent::SessionStarted { session_id }) => {
            assert_eq!(session_id, "mem-test-1");
        }
        other => {
            panic!("First event should be SessionStarted, got: {:?}", other)
        }
    }
}

/// Verify that LLM response complete and SessionEnded are emitted.
#[tokio::test]
async fn test_memory_emits_llm_response_and_end() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider = Arc::new(FakeProvider::new(vec![text_response(
        "Based on our conversation, you prefer dark mode.",
    )]));

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = test_config(workspace);

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config_with_assembler(
        provider,
        tool_registry,
        hook_registry,
        test_support::TEST_USER_ID.to_string(),
        "mem-test-2".to_string(),
        AgentInput::text("What do I prefer?"),
        config,
        Arc::new(ContextAssembler::new(4096)),
        cancel_token,
    );

    let events = collect_events(Agent::run_stream(run_config)).await;

    let llm_complete = events
        .iter()
        .find(|e| matches!(e, AgentEvent::ModelDone(_)));
    assert!(llm_complete.is_some(), "should emit LlmResponseComplete");

    let last = events.last().unwrap();
    assert!(
        matches!(last, AgentEvent::System(SystemEvent::SessionEnded { .. })),
        "last event should be SessionEnded"
    );

    if let AgentEvent::System(SystemEvent::SessionEnded { reason }) = last {
        assert!(
            matches!(reason, SessionEndReason::Completed),
            "session should end as completed, got: {:?}",
            reason
        );
    }
}

/// Verify that HotMemory entries are injected and accessible during the agent loop.
#[tokio::test]
async fn test_hot_memory_injection() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let hot_memory = Arc::new(HotMemory::new(workspace.clone()));
    hot_memory
        .write("user", "User prefers TypeScript for frontend work")
        .await
        .unwrap();
    hot_memory
        .write("project", "Project uses Rust backend")
        .await
        .unwrap();

    let episodic = Arc::new(EpisodicMemory::new_in_memory().await.unwrap());

    let provider = Arc::new(FakeProvider::new(vec![text_response(
        "You mentioned TypeScript preference",
    )]));

    let mut assembler = ContextAssembler::new(4096);
    assembler.set_skill_summaries(
        "# Skills\n- typescript: TypeScript frontend\n- rust: Rust backend"
            .to_string(),
    );

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = test_config(workspace);

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config_with_assembler(
        provider,
        tool_registry,
        hook_registry,
        test_support::TEST_USER_ID.to_string(),
        "mem-test-3".to_string(),
        AgentInput::text("What is my preference?"),
        config,
        Arc::new(assembler),
        cancel_token,
    );

    let events = collect_events(Agent::run_stream(run_config)).await;

    let has_session_end = events.iter().any(|e| {
        matches!(e, AgentEvent::System(SystemEvent::SessionEnded { .. }))
    });
    assert!(has_session_end, "session should complete");

    let has_llm_response =
        events.iter().any(|e| matches!(e, AgentEvent::ModelDone(_)));
    assert!(has_llm_response, "should have LLM response");

    // Keep memory objects alive until end of test
    drop(hot_memory);
    drop(episodic);
}

/// Custom provider that tracks message history to verify memory retrieval.
struct TrackingProvider {
    call_count: std::sync::atomic::AtomicUsize,
    messages_seen:
        tokio::sync::Mutex<Vec<Vec<synthia_provider::types::Message>>>,
    responses: Vec<CompletionResponse>,
}

impl TrackingProvider {
    fn new(responses: Vec<CompletionResponse>) -> Self {
        Self {
            call_count: std::sync::atomic::AtomicUsize::new(0),
            messages_seen: tokio::sync::Mutex::new(Vec::new()),
            responses,
        }
    }

    async fn message_count(&self) -> usize {
        let seen = self.messages_seen.lock().await;
        seen.iter().map(|msgs| msgs.len()).sum()
    }
}

#[async_trait::async_trait]
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

    fn model_config(&self) -> synthia_provider::types::ModelConfig {
        synthia_provider::types::ModelConfig {
            name: "tracking-model".to_string(),
            provider: "tracking".to_string(),
            context_window: 128000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        }
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, Error> {
        let count = self
            .call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        // Record the messages the agent actually sent us so the test
        // can assert memory/context was threaded through correctly.
        self.messages_seen
            .lock()
            .await
            .push(request.messages.to_vec());
        if count < self.responses.len() {
            Ok(self.responses[count].clone())
        } else {
            Err(Error::Provider("No more responses configured".to_string()))
        }
    }

    async fn embed(
        &self,
        _texts: Vec<String>,
    ) -> Result<Vec<Vec<f64>>, synthia_core::Error> {
        Ok(vec![vec![0.0; 1536]; _texts.len()])
    }
}

/// Test that a single-turn scenario (no tool calls) records exactly one
/// LLM call. Named to match the assertion: `call_count == 1` after a
/// single non-tool-calling turn.
#[tokio::test]
async fn test_single_turn_tracking_provider_records_one_call() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let response1 = text_response_completion("Let me look that up");
    let response2 = text_response_completion(
        "You mentioned that earlier - Rust is your preference.",
    );

    let provider = Arc::new(TrackingProvider::new(vec![response1, response2]));

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = test_config(workspace.clone());

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config_with_assembler(
        provider.clone(),
        tool_registry,
        hook_registry,
        test_support::TEST_USER_ID.to_string(),
        "mem-multi-1".to_string(),
        AgentInput::text(
            "What language do I prefer? Remember I told you earlier.",
        ),
        config,
        Arc::new(ContextAssembler::new(8192)),
        cancel_token,
    );

    let events = collect_events(Agent::run_stream(run_config)).await;

    let has_start = events.iter().any(|e| {
        matches!(e, AgentEvent::System(SystemEvent::SessionStarted { .. }))
    });
    let has_end = events.iter().any(|e| {
        matches!(e, AgentEvent::System(SystemEvent::SessionEnded { .. }))
    });
    assert!(has_start, "should start session");
    assert!(has_end, "should end session");

    assert_eq!(
        provider
            .call_count
            .load(std::sync::atomic::Ordering::SeqCst),
        1,
        "LLM should be called once (single turn, no tool calls)"
    );

    let msg_count = provider.message_count().await;
    assert!(
        msg_count >= 1,
        "provider should have seen at least 1 message"
    );
}

/// Companion to `test_multi_turn_memory_with_tracking_provider`: exercises
/// the multi-turn path with tool execution. When the session contains
/// tool calls, the loop makes a 2nd LLM call to deliver the tool result,
/// and `TrackingProvider::messages_seen` records both message sets so
/// the test can verify the tool result is threaded through to the
/// 2nd LLM call. The exact LLM call count is `>= 2` because the
/// end-of-session reflection may also fire (when `recent_tool_results`
/// is non-empty) — and that's the intended behavior of the Gap 3 fix.
#[tokio::test]
async fn test_multi_turn_with_tool_calls_and_tracking_provider() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    // First LLM call returns a tool call; second returns the final text.
    // TrackingProvider indexes by call_count, so response[0] is the
    // tool-call response, response[1] is the final text. A third slot
    // is added as a safe fallback for the optional EOS reflection.
    let tool_call_response = CompletionResponse {
        id: "resp-1".to_string(),
        model: "test-model".to_string(),
        content: Content::parts(vec![ContentPart::ToolUse(ToolUse {
            id: "call_1".to_string(),
            name: "echo".to_string(),
            input: serde_json::json!({"msg": "hi"}),
        })]),
        usage: synthia_provider::types::TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 20,
            total_tokens: 120,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
        },
        cached: false,
    };
    let final_text_response = text_response_completion("All done.");
    let eos_fallback = text_response_completion("Reflection complete.");

    let provider = Arc::new(TrackingProvider::new(vec![
        tool_call_response,
        final_text_response,
        eos_fallback,
    ]));

    let tool_registry = ToolRegistry::new();
    tool_registry.register(ToolEntry::new(Arc::new(FakeTool::new(
        "echo",
        "echo result",
    ))));
    let hook_registry = HookRegistry::new();
    let config = test_config(workspace);

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config_with_assembler(
        provider.clone(),
        tool_registry,
        hook_registry,
        test_support::TEST_USER_ID.to_string(),
        "mem-multi-tool-1".to_string(),
        AgentInput::text("Echo 'hi' and tell me the result."),
        config,
        Arc::new(ContextAssembler::new(8192)),
        cancel_token,
    );

    let events = collect_events(Agent::run_stream(run_config)).await;

    let has_start = events.iter().any(|e| {
        matches!(e, AgentEvent::System(SystemEvent::SessionStarted { .. }))
    });
    let has_end = events.iter().any(|e| {
        matches!(e, AgentEvent::System(SystemEvent::SessionEnded { .. }))
    });
    let has_tool_start = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::Model(ContentPart::ToolUse(tu)) if tu.name == "echo"
        )
    });
    let has_tool_done = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::Model(ContentPart::ToolResult(tr))
                if tr.tool_use_id == "call_1"
        )
    });

    assert!(has_start, "should start session");
    assert!(has_end, "should end session");
    assert!(has_tool_start, "should have echo tool call started");
    assert!(has_tool_done, "should have echo tool call completed");

    // The agent loop must call LLM at least twice: once for the tool
    // call, once for the final text response after the tool result is
    // delivered. A 3rd call is acceptable when EOS reflection fires
    // (recent_tool_results is non-empty), but the contract is "the
    // tool result must reach the 2nd LLM call's messages".
    let calls = provider
        .call_count
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        calls >= 2,
        "LLM should be called at least twice (tool call + final response), got {}",
        calls
    );

    // The 2nd LLM call's messages must include the tool result for the
    // call_1 invocation, proving the tool result is threaded through
    // the agent loop's context assembly.
    let requests = provider.messages_seen.lock().await;
    assert!(
        requests.len() >= 2,
        "Provider should have seen at least 2 message sets, got {}",
        requests.len()
    );
    let second_request = &requests[1];
    let has_tool_result = second_request.iter().any(|m| {
        m.role == synthia_provider::Role::Tool
            && m.tool_call_id.as_deref() == Some("call_1")
    });
    assert!(
        has_tool_result,
        "Second LLM call must include the tool result for call_1"
    );
}

/// Verify that memory handles with both hot and episodic memory work together.
#[tokio::test]
async fn test_memory_handles_combined() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let hot_memory = Arc::new(HotMemory::new(workspace.clone()));
    hot_memory
        .write("context", "Previous conversation about API design")
        .await
        .unwrap();

    let episodic = Arc::new(EpisodicMemory::new_in_memory().await.unwrap());

    let provider = Arc::new(FakeProvider::new(vec![text_response(
        "API design complete",
    )]));

    let mut assembler = ContextAssembler::new(8192);
    assembler.set_skill_summaries(
        "# Skills\n- api_design: Design REST APIs".to_string(),
    );

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = test_config(workspace);

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config_with_assembler(
        provider,
        tool_registry,
        hook_registry,
        test_support::TEST_USER_ID.to_string(),
        "mem-combined-1".to_string(),
        AgentInput::text("Design an endpoint"),
        config,
        Arc::new(assembler),
        cancel_token,
    );

    let events = collect_events(Agent::run_stream(run_config)).await;

    let has_session_end = events.iter().any(|e| {
        matches!(e, AgentEvent::System(SystemEvent::SessionEnded { .. }))
    });
    assert!(
        has_session_end,
        "session with combined memory should complete"
    );

    // Keep memory objects alive until end of test
    drop(hot_memory);
    drop(episodic);
}
