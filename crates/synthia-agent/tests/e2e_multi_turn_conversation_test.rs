use std::sync::Arc;

use futures::StreamExt;
use synthia_agent::{agent::Agent, config::AgentConfig, types::AgentEvent};
use synthia_hook::HookRegistry;
use synthia_tool::registry::{ToolEntry, ToolRegistry};
use tokio_util::sync::CancellationToken;

mod test_support;
use test_support::{
    FakeProvider,
    FakeTool,
    create_test_workspace,
    make_run_config,
};

fn create_test_tool_registry() -> ToolRegistry {
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(FakeTool::new(
        "read_file",
        "file content here",
    ))));
    registry
}

#[tokio::test]
async fn test_two_turn_conversation() {
    let workspace = create_test_workspace();

    let provider = Arc::new(FakeProvider::new(vec![
        "First response".to_string(),
        "Second response".to_string(),
    ]));

    let agent_config = AgentConfig {
        model: "fake-model".to_string(),
        max_iterations: 10,
        max_tokens: 1024,
        temperature: Some(0.7),
        workspace_root: workspace,
        ..Default::default()
    };

    let cancel_token = CancellationToken::new();

    let run_config = make_run_config(
        provider,
        create_test_tool_registry(),
        HookRegistry::new(),
        "multi-turn-test".to_string(),
        synthia_agent::types::AgentInput::text("First message"),
        agent_config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let session_started = events
        .iter()
        .any(|e| matches!(e, AgentEvent::SessionStarted { .. }));
    let llm_responses = events
        .iter()
        .filter(|e| matches!(e, AgentEvent::LlmResponseComplete { .. }))
        .count();
    let session_ended = events
        .iter()
        .any(|e| matches!(e, AgentEvent::SessionEnded { .. }));

    assert!(session_started, "Session should start");
    assert!(llm_responses >= 1, "Should have at least one LLM response");
    assert!(session_ended, "Session should end");
}

#[tokio::test]
async fn test_three_turn_conversation() {
    let workspace = create_test_workspace();

    let turn1_provider =
        Arc::new(FakeProvider::new(vec!["Turn 1 response".to_string()]));
    let turn2_provider =
        Arc::new(FakeProvider::new(vec!["Turn 2 response".to_string()]));
    let turn3_provider =
        Arc::new(FakeProvider::new(vec!["Turn 3 response".to_string()]));

    let agent_config = AgentConfig {
        model: "fake-model".to_string(),
        max_iterations: 15,
        max_tokens: 1024,
        temperature: Some(0.7),
        workspace_root: workspace,
        ..Default::default()
    };

    let cancel_token = CancellationToken::new();

    let turn1_config = make_run_config(
        turn1_provider,
        create_test_tool_registry(),
        HookRegistry::new(),
        "three-turn-test".to_string(),
        synthia_agent::types::AgentInput::text("Turn 1 message"),
        agent_config.clone(),
        cancel_token.clone(),
    );

    let events: Vec<AgentEvent> =
        Agent::run_stream(turn1_config).collect().await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::SessionStarted { .. })),
        "Turn 1 should start a session"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::LlmResponseComplete { .. })),
        "Turn 1 should complete"
    );

    let turn2_config = make_run_config(
        turn2_provider,
        create_test_tool_registry(),
        HookRegistry::new(),
        "three-turn-test".to_string(),
        synthia_agent::types::AgentInput::text("Turn 2 message"),
        agent_config.clone(),
        cancel_token.clone(),
    );

    let events: Vec<AgentEvent> =
        Agent::run_stream(turn2_config).collect().await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::LlmResponseComplete { .. })),
        "Turn 2 should complete"
    );

    let turn3_config = make_run_config(
        turn3_provider,
        create_test_tool_registry(),
        HookRegistry::new(),
        "three-turn-test".to_string(),
        synthia_agent::types::AgentInput::text("Turn 3 message"),
        agent_config,
        cancel_token,
    );

    let events: Vec<AgentEvent> =
        Agent::run_stream(turn3_config).collect().await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::SessionEnded { .. })),
        "Turn 3 should end the session"
    );
}
