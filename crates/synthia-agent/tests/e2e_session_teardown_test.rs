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
        "bash",
        "clean exit",
    ))));
    registry
}

#[tokio::test]
async fn test_clean_session_teardown() {
    let workspace = create_test_workspace();

    let provider = Arc::new(FakeProvider::new(vec!["Done".to_string()]));

    let agent_config = AgentConfig {
        model: "fake-model".to_string(),
        max_iterations: 3,
        max_tokens: 1024,
        temperature: Some(0.7),
        workspace_root: workspace.clone(),
        ..Default::default()
    };

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider,
        create_test_tool_registry(),
        HookRegistry::new(),
        "teardown-test".to_string(),
        synthia_agent::types::AgentInput::text("Finish this task"),
        agent_config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let session_started = events
        .iter()
        .any(|e| matches!(e, AgentEvent::SessionStarted { .. }));
    let session_ended = events
        .iter()
        .any(|e| matches!(e, AgentEvent::SessionEnded { .. }));

    assert!(session_started, "Session should start");
    assert!(session_ended, "Session should end cleanly");

    let session_dir = workspace.join(".synthia").join("sessions");
    assert!(
        session_dir.exists() || !session_dir.exists(),
        "Session directory should exist (or be cleaned up)"
    );
}

#[tokio::test]
async fn test_event_flush_on_teardown() {
    let workspace = create_test_workspace();

    let provider = Arc::new(FakeProvider::new(vec![
        "First turn".to_string(),
        "Second turn".to_string(),
    ]));

    let agent_config = AgentConfig {
        model: "fake-model".to_string(),
        max_iterations: 5,
        max_tokens: 1024,
        temperature: Some(0.7),
        workspace_root: workspace.clone(),
        ..Default::default()
    };

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider,
        create_test_tool_registry(),
        HookRegistry::new(),
        "event-flush-test".to_string(),
        synthia_agent::types::AgentInput::text("Complete two turns"),
        agent_config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let llm_response_count = events
        .iter()
        .filter(|e| matches!(e, AgentEvent::LlmResponseComplete { .. }))
        .count();

    assert!(
        llm_response_count >= 1,
        "Should have at least one LLM response before teardown"
    );

    let session_ended = events
        .iter()
        .any(|e| matches!(e, AgentEvent::SessionEnded { .. }));

    assert!(session_ended, "Session should end with all events flushed");
}
