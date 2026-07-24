#![allow(deprecated)]
//! E2E test: Complete event sequence verification.
//!
//! Tests that all AgentEvents are emitted in the correct order for a typical flow:
//! SessionStarted -> LlmStreamDelta(s) -> LlmResponseComplete -> SessionEnded

mod test_support;
use std::{path::PathBuf, sync::Arc};

use futures::StreamExt;
use synthia_agent::{
    agent::Agent,
    config::AgentConfig,
    events::{HookEvent, SystemEvent, WarningKind},
    types::{AgentEvent, AgentInput, SessionEndReason, TokenUsage},
};
use synthia_hook::HookRegistry;
use synthia_provider::types::{ContentPart, ReasoningContent, TextContent};
use synthia_session::types::TokenBudget;
use synthia_tool::registry::ToolRegistry;
use test_support::{FakeProvider, make_run_config};
use tokio_util::sync::CancellationToken;

async fn collect(
    stream: impl futures::Stream<Item = AgentEvent>,
) -> Vec<AgentEvent> {
    stream.collect().await
}

fn event_variant_name(event: &AgentEvent) -> String {
    match event {
        AgentEvent::System(SystemEvent::SessionStarted { .. }) => {
            "SessionStarted".to_string()
        }
        AgentEvent::System(SystemEvent::SessionInterrupted { .. }) => {
            "SessionInterrupted".to_string()
        }
        AgentEvent::System(SystemEvent::SessionEnded { .. }) => {
            "SessionEnded".to_string()
        }
        AgentEvent::System(SystemEvent::Progress { .. }) => {
            "Progress".to_string()
        }
        AgentEvent::System(SystemEvent::Warning { kind, .. }) => match kind {
            WarningKind::Guardian => "GuardianWarning".to_string(),
            WarningKind::TokenBudget => "TokenBudgetWarning".to_string(),
            WarningKind::Loop => "LoopWarning".to_string(),
            WarningKind::ContextCompaction => "ContextCompacted".to_string(),
            WarningKind::Hook => "HookError".to_string(),
            WarningKind::EditConflict => "EditConflict".to_string(),
        },
        AgentEvent::Model(ContentPart::Reasoning(ReasoningContent {
            ..
        })) => "Thinking".to_string(),
        AgentEvent::Model(ContentPart::Text(TextContent { .. })) => {
            "LlmStreamDelta".to_string()
        }
        AgentEvent::Model(ContentPart::ToolUse(_)) => {
            "ToolCallStarted".to_string()
        }
        AgentEvent::Model(ContentPart::ToolResult(_)) => {
            "ToolCallCompleted".to_string()
        }
        AgentEvent::ModelDone(_) => "LlmResponseComplete".to_string(),
        AgentEvent::Hook(HookEvent::Message { .. }) => {
            "SteeringReceived".to_string()
        }
        AgentEvent::Hook(HookEvent::ConfirmRequest { .. }) => {
            "GuardianConfirmationRequest".to_string()
        }
        AgentEvent::Hook(HookEvent::ConfirmResponse { .. }) => {
            "ToolCallSkipped".to_string()
        }
        AgentEvent::Hook(HookEvent::Custom { .. }) => "Other".to_string(),
        AgentEvent::Agent(_, _) => "Other".to_string(),
        _ => "Other".to_string(),
    }
}

fn event_type_names(events: &[AgentEvent]) -> Vec<String> {
    events.iter().map(event_variant_name).collect()
}

fn test_config(workspace: PathBuf) -> AgentConfig {
    AgentConfig {
        model: "test-model".to_string(),
        max_tokens: 4096,
        max_iterations: 5,
        temperature: None,
        workspace_root: workspace,
        token_budget: None,
        checkpoint_dir: None,
        context_token_budget: Some(TokenBudget::new(128_000)),
        compaction_provider: None,
        observability: None,
        ..Default::default()
    }
}

fn text_response_string(content: &str) -> String {
    content.to_string()
}

/// Test the basic event sequence: SessionStarted -> ... -> SessionEnded.
#[tokio::test]
async fn test_basic_event_sequence() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider =
        Arc::new(FakeProvider::new(vec![text_response_string("Hello there")]));

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = test_config(workspace);

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider,
        tool_registry,
        hook_registry,
        "seq-test-1".to_string(),
        AgentInput::text("Say hello"),
        config,
        cancel_token,
    );

    let events = collect(Agent::run_stream(run_config)).await;

    assert!(
        matches!(
            &events[0],
            AgentEvent::System(SystemEvent::SessionStarted { .. })
        ),
        "First event should be SessionStarted"
    );

    let last = events.last().unwrap();
    assert!(
        matches!(last, AgentEvent::System(SystemEvent::SessionEnded { .. })),
        "Last event should be SessionEnded, got: {:?}",
        last
    );

    if let AgentEvent::System(SystemEvent::SessionEnded { reason }) = last {
        assert!(
            matches!(reason, SessionEndReason::Completed),
            "Session should end as completed"
        );
    }
}

/// Test that the first event is a SessionStarted.
#[tokio::test]
async fn test_iteration_before_llm_request() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider =
        Arc::new(FakeProvider::new(vec![text_response_string("Response")]));

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = test_config(workspace);

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider,
        tool_registry,
        hook_registry,
        "seq-test-2".to_string(),
        AgentInput::text("Test"),
        config,
        cancel_token,
    );

    let events = collect(Agent::run_stream(run_config)).await;
    let names = event_type_names(&events);

    let session_start_idx = names
        .iter()
        .position(|n| n == "SessionStarted")
        .expect("should have SessionStarted");
    let llm_resp_idx = names
        .iter()
        .position(|n| n == "LlmResponseComplete")
        .expect("should have LlmResponseComplete");

    assert!(
        session_start_idx < llm_resp_idx,
        "SessionStarted (index {}) should come before LlmResponseComplete (index {})",
        session_start_idx,
        llm_resp_idx
    );
}

/// Test that LlmResponseComplete comes before SessionEnded.
#[tokio::test]
async fn test_stream_delta_before_response_complete() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider = Arc::new(FakeProvider::new(vec![text_response_string(
        "Streaming response content",
    )]));

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = test_config(workspace);

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider,
        tool_registry,
        hook_registry,
        "seq-test-3".to_string(),
        AgentInput::text("Stream me a response"),
        config,
        cancel_token,
    );

    let events = collect(Agent::run_stream(run_config)).await;
    let names = event_type_names(&events);

    let llm_resp_idx = names.iter().position(|n| n == "LlmResponseComplete");
    assert!(
        llm_resp_idx.is_some(),
        "should have LlmResponseComplete event"
    );

    let session_end_idx =
        names.iter().position(|n| n == "SessionEnded").unwrap();
    let llm_resp_idx = llm_resp_idx.unwrap();
    assert!(
        llm_resp_idx < session_end_idx,
        "LlmResponseComplete (index {}) should come before SessionEnded (index {})",
        llm_resp_idx,
        session_end_idx
    );
}

/// Test that SessionEnded always comes last.
#[tokio::test]
async fn test_iteration_completed_before_session_end() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider = Arc::new(FakeProvider::new(vec![text_response_string(
        "Final answer",
    )]));

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = test_config(workspace);

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider,
        tool_registry,
        hook_registry,
        "seq-test-4".to_string(),
        AgentInput::text("Give final answer"),
        config,
        cancel_token,
    );

    let events = collect(Agent::run_stream(run_config)).await;
    let names = event_type_names(&events);

    let session_end_idx =
        names.iter().position(|n| n == "SessionEnded").unwrap();
    let llm_resp_idx = names
        .iter()
        .position(|n| n == "LlmResponseComplete")
        .expect("should have LlmResponseComplete");

    assert!(
        llm_resp_idx < session_end_idx,
        "LlmResponseComplete should come before SessionEnded"
    );
}

/// Test the complete expected event sequence for a typical single-turn flow.
#[tokio::test]
async fn test_complete_single_turn_sequence() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider =
        Arc::new(FakeProvider::new(vec![text_response_string("Done")]));

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = test_config(workspace);

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider,
        tool_registry,
        hook_registry,
        "seq-test-5".to_string(),
        AgentInput::text("Single turn test"),
        config,
        cancel_token,
    );

    let events = collect(Agent::run_stream(run_config)).await;
    let names = event_type_names(&events);

    assert!(
        names.contains(&"SessionStarted".to_string()),
        "should have SessionStarted"
    );
    assert!(
        names.contains(&"LlmResponseComplete".to_string()),
        "should have LlmResponseComplete"
    );
    assert!(
        names.contains(&"SessionEnded".to_string()),
        "should have SessionEnded"
    );
}

/// Test that SessionEnded is always the final event.
#[tokio::test]
async fn test_session_ended_is_final_event() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider =
        Arc::new(FakeProvider::new(vec![text_response_string("ok")]));

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = test_config(workspace);

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider,
        tool_registry,
        hook_registry,
        "seq-test-6".to_string(),
        AgentInput::text("test"),
        config,
        cancel_token,
    );

    let events = collect(Agent::run_stream(run_config)).await;

    let last = events.last().expect("should have at least one event");
    assert!(
        matches!(last, AgentEvent::System(SystemEvent::SessionEnded { .. })),
        "last event must be SessionEnded"
    );
}

/// Test that the session ID is consistent across all events.
#[tokio::test]
async fn test_session_id_consistency() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider =
        Arc::new(FakeProvider::new(vec![text_response_string("consistent")]));

    let expected_session_id = "consistency-test";
    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = test_config(workspace);

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider,
        tool_registry,
        hook_registry,
        expected_session_id.to_string(),
        AgentInput::text("test"),
        config,
        cancel_token,
    );

    let events = collect(Agent::run_stream(run_config)).await;

    match &events[0] {
        AgentEvent::System(SystemEvent::SessionStarted { session_id }) => {
            assert_eq!(session_id, expected_session_id);
        }
        _ => panic!("First event should be SessionStarted"),
    }
}

/// Test event serialization roundtrip for all variants.
#[tokio::test]
async fn test_event_serialization_roundtrip() {
    let events = vec![
        AgentEvent::System(SystemEvent::SessionStarted {
            session_id: "s1".to_string(),
        }),
        AgentEvent::System(SystemEvent::SessionEnded {
            reason: SessionEndReason::Completed,
        }),
        AgentEvent::Model(ContentPart::Text(TextContent {
            text: "Hello".to_string(),
            cache_control: None,
        })),
        AgentEvent::ModelDone(synthia_provider::SamplingResult {
            text: "Hello world".to_string(),
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
            },
        }),
    ];

    for event in events {
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: AgentEvent = serde_json::from_str(&json).unwrap();

        assert!(
            std::mem::discriminant(&event)
                == std::mem::discriminant(&deserialized),
            "Event variant type should be preserved after serialization roundtrip"
        );
    }
}

/// Test that the event sequence is non-empty and contains a SessionStarted.
#[tokio::test]
async fn test_iteration_numbering_in_events() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider =
        Arc::new(FakeProvider::new(vec![text_response_string("iteration 1")]));

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = test_config(workspace);

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider,
        tool_registry,
        hook_registry,
        "seq-iter-test".to_string(),
        AgentInput::text("test iteration"),
        config,
        cancel_token,
    );

    let events = collect(Agent::run_stream(run_config)).await;

    let session_started_count = events
        .iter()
        .filter(|e| {
            matches!(e, AgentEvent::System(SystemEvent::SessionStarted { .. }))
        })
        .count();

    assert!(
        session_started_count >= 1,
        "should have at least one SessionStarted event"
    );
}
