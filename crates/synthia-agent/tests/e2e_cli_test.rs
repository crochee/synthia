//! E2E test: CLI input flowing through to ReAct loop.
//!
//! Tests the CLI command structures and their interaction with the agent.
//! Verifies that CLI input is properly formatted and flows through to the ReAct loop.

mod test_support;
use std::{path::PathBuf, sync::Arc};

use futures::StreamExt;
use synthia_agent::{agent::Agent, config::AgentConfig, types::AgentEvent};
use synthia_command::CommandRegistry;
use synthia_hook::HookRegistry;
use synthia_session::types::TokenBudget;
use synthia_tool::registry::ToolRegistry;
use test_support::{FakeProvider, make_run_config};
use tokio_util::sync::CancellationToken;

/// Helper to collect events from the agent stream.
async fn collect(
    stream: impl futures::Stream<Item = AgentEvent>,
) -> Vec<AgentEvent> {
    stream.collect().await
}

fn test_config(workspace: PathBuf) -> AgentConfig {
    AgentConfig {
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
    }
}

fn text_response(content: &str) -> String {
    content.to_string()
}

/// Test that CLI-style text input flows through to the ReAct loop correctly.
#[tokio::test]
async fn test_cli_input_flows_to_react_loop() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider = Arc::new(FakeProvider::new(vec![text_response(
        "I processed your CLI input.",
    )]));

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = test_config(workspace.clone());

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider,
        tool_registry,
        hook_registry,
        "cli-test-1".to_string(),
        synthia_agent::types::AgentInput::text(
            "list all files in the current directory",
        ),
        config,
        cancel_token,
    );

    let events = collect(Agent::run_stream(run_config)).await;

    let has_start = events
        .iter()
        .any(|e| matches!(e, AgentEvent::SessionStarted { .. }));
    let has_end = events
        .iter()
        .any(|e| matches!(e, AgentEvent::SessionEnded { .. }));
    assert!(has_start, "should start session");
    assert!(has_end, "should end session");

    let llm_response = events
        .iter()
        .find(|e| matches!(e, AgentEvent::LlmResponseComplete { .. }));
    assert!(llm_response.is_some(), "LLM should have responded");
}

/// Test that checkpoint save occurs after command completion.
#[tokio::test]
async fn test_checkpoint_save_after_completion() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider =
        Arc::new(FakeProvider::new(vec![text_response("Task complete.")]));

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = test_config(workspace.clone());

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider,
        tool_registry,
        hook_registry,
        "cli-test-2".to_string(),
        synthia_agent::types::AgentInput::text("Do a simple task"),
        config,
        cancel_token,
    );

    let events = collect(Agent::run_stream(run_config)).await;

    let has_end = events
        .iter()
        .any(|e| matches!(e, AgentEvent::SessionEnded { .. }));
    assert!(has_end, "session should complete");

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let session_dir = workspace
        .join(".synthia")
        .join("sessions")
        .join(test_support::TEST_USER_ID)
        .join("cli-test-2");
    assert!(
        session_dir.exists(),
        "session directory should exist at {:?}",
        session_dir
    );
}

/// Test that the agent correctly handles a multi-word CLI command.
#[tokio::test]
async fn test_multiline_cli_input() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider = Arc::new(FakeProvider::new(vec![text_response(
        "Multi-line input received.",
    )]));

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = test_config(workspace.clone());

    let multiline_input = "Search for all Rust files\n\
        Then list their dependencies\n\
        And summarize the results";

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider,
        tool_registry,
        hook_registry,
        "cli-test-3".to_string(),
        synthia_agent::types::AgentInput::text(multiline_input),
        config,
        cancel_token,
    );

    let events = collect(Agent::run_stream(run_config)).await;

    let llm_response = events
        .iter()
        .find(|e| matches!(e, AgentEvent::LlmResponseComplete { .. }));
    assert!(
        llm_response.is_some(),
        "LLM should respond to multiline input"
    );

    let has_session_end = events
        .iter()
        .any(|e| matches!(e, AgentEvent::SessionEnded { .. }));
    assert!(
        has_session_end,
        "session should complete for multiline input"
    );
}

/// Test that slash commands in CLI input are handled.
#[tokio::test]
async fn test_slash_command_in_cli_input() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider = Arc::new(FakeProvider::new(vec![text_response("ok")]));

    let command_registry = CommandRegistry::new();
    command_registry.register_builtins();

    let ctx = synthia_command::types::CommandContext::new(
        "cli-test-4".to_string(),
        workspace.clone(),
    );

    let result = command_registry.dispatch("/help", &ctx).await;
    assert!(result.is_ok(), "/help command should execute");

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = test_config(workspace.clone());

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider,
        tool_registry,
        hook_registry,
        "cli-test-4".to_string(),
        synthia_agent::types::AgentInput::text("/help"),
        config,
        cancel_token,
    );

    let events = collect(Agent::run_stream(run_config)).await;

    let has_session_end = events
        .iter()
        .any(|e| matches!(e, AgentEvent::SessionEnded { .. }));
    assert!(has_session_end, "session should complete for slash command");
}

/// Test that CLI input with special characters is handled properly.
#[tokio::test]
async fn test_special_characters_in_cli_input() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();

    let provider = Arc::new(FakeProvider::new(vec![text_response(
        "processed special chars",
    )]));

    let tool_registry = ToolRegistry::new();
    let hook_registry = HookRegistry::new();
    let config = test_config(workspace.clone());

    let special_input =
        "Find files with pattern: *.rs and grep for 'fn main()'";

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider,
        tool_registry,
        hook_registry,
        "cli-test-5".to_string(),
        synthia_agent::types::AgentInput::text(special_input),
        config,
        cancel_token,
    );

    let events = collect(Agent::run_stream(run_config)).await;

    let has_session_end = events
        .iter()
        .any(|e| matches!(e, AgentEvent::SessionEnded { .. }));
    assert!(
        has_session_end,
        "session should complete with special characters in input"
    );
}
