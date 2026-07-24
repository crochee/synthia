#![allow(deprecated)]
//! E2E test: Steering injection during active ReAct loop.

mod test_support;
use std::sync::Arc;

use futures::StreamExt;
use synthia_agent::{
    agent::Agent,
    config::AgentConfig,
    events::{HookEvent, SystemEvent},
    steering::{MpscSteeringChannel, SteeringChannel, SteeringMessage},
    types::{AgentEvent, AgentInput},
};
use synthia_context::ContextAssembler;
use synthia_hook::HookRegistry;
use synthia_session::types::TokenBudget;
use synthia_tool::registry::ToolRegistry;
use test_support::{FakeProvider, make_run_config_with_steering};
use tokio_util::sync::CancellationToken;

fn text_response(content: &str) -> String {
    content.to_string()
}

#[tokio::test]
async fn test_steering_message_emitted_as_event() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider = Arc::new(FakeProvider::new(vec![text_response(
        "I will focus on testing.",
    )]));

    let steering_channel = Arc::new(MpscSteeringChannel::new());
    steering_channel
        .send(SteeringMessage::new("focus on testing"))
        .await;

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = AgentConfig {
        model: "test-model".to_string(),
        max_tokens: 4096,
        max_iterations: 3,
        temperature: None,
        workspace_root: workspace,
        token_budget: None,
        checkpoint_dir: None,
        context_token_budget: Some(TokenBudget::new(128_000)),
        observability: None,
        compaction_provider: None,
        ..Default::default()
    };

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config_with_steering(
        provider,
        tool_registry,
        hook_registry,
        test_support::TEST_USER_ID.to_string(),
        "steer-test-1".to_string(),
        AgentInput::text("Hello"),
        config,
        Arc::new(ContextAssembler::new(4096)),
        cancel_token,
        steering_channel,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let steering_event = events
        .iter()
        .find(|e| matches!(e, AgentEvent::Hook(HookEvent::Message { .. })));
    assert!(
        steering_event.is_some(),
        "should emit Hook::Message event when pending input has a message"
    );

    if let Some(AgentEvent::Hook(HookEvent::Message { message, .. })) =
        steering_event
    {
        assert_eq!(message, "focus on testing");
    }
}

#[tokio::test]
async fn test_multiple_steering_messages() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider = Arc::new(FakeProvider::new(vec![
        text_response("Understood, focusing on testing."),
        text_response("Also focusing on performance."),
    ]));

    let steering_channel = Arc::new(MpscSteeringChannel::new());
    steering_channel
        .send(SteeringMessage::new("focus on testing"))
        .await;
    steering_channel
        .send(SteeringMessage::new("also focus on performance"))
        .await;

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = AgentConfig {
        model: "test-model".to_string(),
        max_tokens: 4096,
        max_iterations: 3,
        temperature: None,
        workspace_root: workspace,
        token_budget: None,
        checkpoint_dir: None,
        context_token_budget: Some(TokenBudget::new(128_000)),
        observability: None,
        compaction_provider: None,
        ..Default::default()
    };

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config_with_steering(
        provider,
        tool_registry,
        hook_registry,
        test_support::TEST_USER_ID.to_string(),
        "steer-test-2".to_string(),
        AgentInput::text("Hello"),
        config,
        Arc::new(ContextAssembler::new(4096)),
        cancel_token,
        steering_channel,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let steering_count = events
        .iter()
        .filter(|e| matches!(e, AgentEvent::Hook(HookEvent::Message { .. })))
        .count();
    assert!(
        steering_count >= 1,
        "should emit at least 1 Hook::Message event, got {}",
        steering_count
    );
}

#[tokio::test]
async fn test_steering_message_emits_steering_received_event() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider = Arc::new(FakeProvider::new(vec![text_response("Done")]));

    let steering_channel = Arc::new(MpscSteeringChannel::new());
    steering_channel
        .send(SteeringMessage::new("steering instruction"))
        .await;

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = AgentConfig {
        model: "test-model".to_string(),
        max_tokens: 4096,
        max_iterations: 3,
        temperature: None,
        workspace_root: workspace,
        token_budget: None,
        checkpoint_dir: None,
        context_token_budget: Some(TokenBudget::new(128_000)),
        observability: None,
        compaction_provider: None,
        ..Default::default()
    };

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config_with_steering(
        provider,
        tool_registry,
        hook_registry,
        test_support::TEST_USER_ID.to_string(),
        "steer-test-3".to_string(),
        AgentInput::text("Start task"),
        config,
        Arc::new(ContextAssembler::new(4096)),
        cancel_token,
        steering_channel,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let has_start = events.iter().any(|e| {
        matches!(e, AgentEvent::System(SystemEvent::SessionStarted { .. }))
    });
    let has_end = events.iter().any(|e| {
        matches!(e, AgentEvent::System(SystemEvent::SessionEnded { .. }))
    });
    assert!(has_start, "should start session");
    assert!(has_end, "should end session");

    let steering_count = events
        .iter()
        .filter(|e| matches!(e, AgentEvent::Hook(HookEvent::Message { .. })))
        .count();
    assert!(steering_count >= 1, "should have received steering message");
}

#[tokio::test]
async fn test_no_steering_when_empty_queue() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider =
        Arc::new(FakeProvider::new(vec![text_response("Hello back")]));

    let steering_channel = Arc::new(MpscSteeringChannel::new());

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = AgentConfig {
        model: "test-model".to_string(),
        max_tokens: 4096,
        max_iterations: 3,
        temperature: None,
        workspace_root: workspace,
        token_budget: None,
        checkpoint_dir: None,
        context_token_budget: Some(TokenBudget::new(128_000)),
        observability: None,
        compaction_provider: None,
        ..Default::default()
    };

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config_with_steering(
        provider,
        tool_registry,
        hook_registry,
        test_support::TEST_USER_ID.to_string(),
        "steer-test-4".to_string(),
        AgentInput::text("Hello"),
        config,
        Arc::new(ContextAssembler::new(4096)),
        cancel_token,
        steering_channel,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let steering_count = events
        .iter()
        .filter(|e| matches!(e, AgentEvent::Hook(HookEvent::Message { .. })))
        .count();
    assert_eq!(
        steering_count, 0,
        "should have no Hook::Message events when queue is empty"
    );
}

#[tokio::test]
async fn test_steering_content_preserved() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider = Arc::new(FakeProvider::new(vec![text_response("ok")]));

    let steering_channel = Arc::new(MpscSteeringChannel::new());
    steering_channel
        .send(SteeringMessage::new("Please prioritize security checks"))
        .await;

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = AgentConfig {
        model: "test-model".to_string(),
        max_tokens: 4096,
        max_iterations: 3,
        temperature: None,
        workspace_root: workspace,
        token_budget: None,
        checkpoint_dir: None,
        context_token_budget: Some(TokenBudget::new(128_000)),
        observability: None,
        compaction_provider: None,
        ..Default::default()
    };

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config_with_steering(
        provider,
        tool_registry,
        hook_registry,
        test_support::TEST_USER_ID.to_string(),
        "steer-test-5".to_string(),
        AgentInput::text("Begin"),
        config,
        Arc::new(ContextAssembler::new(4096)),
        cancel_token,
        steering_channel,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let steering_event = events
        .iter()
        .find(|e| matches!(e, AgentEvent::Hook(HookEvent::Message { .. })));
    assert!(steering_event.is_some(), "should have steering event");

    if let Some(AgentEvent::Hook(HookEvent::Message { message, .. })) =
        steering_event
    {
        assert_eq!(message, "Please prioritize security checks");
    }
}
