#![allow(deprecated)]
//! Integration tests for sub-agent execution via `AgentTool`.
//!
//! These tests verify the sub-agent spawn plumbing: foreground mode
//! (await result), background mode (return immediately), and error
//! handling (missing parent config, depth limit, concurrency limit).

mod test_support;

use std::sync::Arc;

use synthia_agent::{
    config::{AgentConfig, AgentRunConfigBuilder},
    control::{AgentControl, AgentRegistry},
    tools::agent_tools::{agent_tool::AgentTool, team::SubagentManager},
    types::AgentInput,
};
use synthia_context::ContextAssembler;
use synthia_hook::HookRegistry;
use synthia_provider::router::ModelRouter;
use synthia_session::Store as SessionStore;
use synthia_tool::{
    traits::Tool,
    types::{ToolExecutionContext, ToolInput},
};

fn make_tool_input(prompt: &str) -> ToolInput {
    ToolInput {
        name: "task".to_string(),
        input: serde_json::json!({
            "description": "test task",
            "prompt": prompt,
            "subagent_type": "general",
        }),
        context: ToolExecutionContext::new(
            "test-session".to_string(),
            std::path::PathBuf::from("/tmp"),
        ),
    }
}

fn make_minimal_run_config()
-> (synthia_agent::config::AgentRunConfig, tempfile::TempDir) {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().to_path_buf();
    let session_store =
        SessionStore::new(workspace.join(".synthia").join("sessions"));

    let config = AgentRunConfigBuilder::new()
        .provider(Arc::new(
            test_support::FakeProvider::new(vec!["ok".to_string()])
                .with_response("ok"),
        ))
        .tool_registry(synthia_tool::registry::ToolRegistry::new())
        .hook_registry(Arc::new(HookRegistry::new()))
        .model_router(Arc::new(ModelRouter::new()))
        .user_id("test-user".to_string())
        .session_id("test-session".to_string())
        .input(AgentInput::text("test"))
        .config(AgentConfig {
            model: "test-model".to_string(),
            max_tokens: 4096,
            max_iterations: 2,
            temperature: None,
            workspace_root: workspace,
            ..Default::default()
        })
        .context_assembler(Arc::new(ContextAssembler::new(4096)))
        .session_store(session_store)
        .cancel_token(tokio_util::sync::CancellationToken::new())
        .subagent_session_factory(Arc::new(
            test_support::FakeSubagentFactory::default(),
        ))
        .agent_control(AgentControl::new(Arc::new(AgentRegistry::new())))
        .build()
        .unwrap();

    (config, temp)
}

#[tokio::test]
async fn test_subagent_foreground_mode_returns_result() {
    // Verify that foreground mode (default) spawns a sub-agent and
    // collects the result through the oneshot channel.
    let manager = Arc::new(SubagentManager::new());
    let (config, _temp) = make_minimal_run_config();
    manager.set_parent_config(config);
    let tool = AgentTool::new(manager, true);

    let input = make_tool_input("return the answer 42");
    let output = tool.call(input).await;

    assert!(output.is_text(), "expected text output, got error");
    let text = output
        .content
        .first()
        .and_then(|p| match p {
            synthia_provider::types::ContentPart::Text(t) => {
                Some(t.text.clone())
            }
            _ => None,
        })
        .unwrap_or_default();
    assert!(
        text.contains("Sub-agent completed"),
        "expected 'Sub-agent completed' in output, got: {text}"
    );
    assert!(
        text.contains("return the answer 42"),
        "expected prompt in output, got: {text}"
    );
}

#[tokio::test]
async fn test_subagent_background_mode_returns_immediately() {
    // Verify that background mode spawns a sub-agent and returns
    // immediately with a status message.
    let manager = Arc::new(SubagentManager::new());
    let (config, _temp) = make_minimal_run_config();
    manager.set_parent_config(config);
    let tool = AgentTool::new(manager, true);

    let mut input = make_tool_input("do background work");
    input
        .input
        .as_object_mut()
        .unwrap()
        .insert("background".to_string(), serde_json::Value::Bool(true));

    let output = tool.call(input).await;

    assert!(output.is_text(), "expected text output, got error");
    let text = output
        .content
        .first()
        .and_then(|p| match p {
            synthia_provider::types::ContentPart::Text(t) => {
                Some(t.text.clone())
            }
            _ => None,
        })
        .unwrap_or_default();
    assert!(
        text.contains("Sub-agent spawned in background"),
        "expected background spawn message, got: {text}"
    );
}

#[tokio::test]
async fn test_subagent_background_mode_registers_with_agent_control() {
    // Verify that a background task is tracked by AgentControl and that
    // check_completed eventually returns its result.
    let manager = Arc::new(SubagentManager::new());
    let (config, _temp) = make_minimal_run_config();
    let control = config.agent_control.clone().unwrap();
    manager.set_parent_config(config);
    let tool = AgentTool::new(manager, true);

    let mut input = make_tool_input("do background work");
    input
        .input
        .as_object_mut()
        .unwrap()
        .insert("background".to_string(), serde_json::Value::Bool(true));

    let output = tool.call(input).await;
    assert!(output.is_text(), "expected text output, got error");

    // Poll AgentControl until the task completes.
    let mut completed = Vec::new();
    for _ in 0..50 {
        completed = control.check_completed().await;
        if !completed.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    assert_eq!(completed.len(), 1, "expected one completed background task");
    let task = &completed[0];
    assert_eq!(
        task.status,
        synthia_agent::agent_instance::AgentStatus::Completed
    );
    assert!(
        task.output.contains("do background work"),
        "expected output to contain prompt, got: {}",
        task.output
    );
}

#[tokio::test]
async fn test_subagent_missing_parent_config_returns_error() {
    // Verify that calling the Agent tool without parent_config set
    // returns an error.
    let manager = Arc::new(SubagentManager::new());
    let tool = AgentTool::new(manager, true);

    let input = make_tool_input("do something");
    let output = tool.call(input).await;

    assert!(!output.is_text(), "expected error output");
    let text = output
        .content
        .first()
        .and_then(|p| match p {
            synthia_provider::types::ContentPart::Text(t) => {
                Some(t.text.clone())
            }
            _ => None,
        })
        .unwrap_or_default();
    assert!(
        text.contains("parent config"),
        "expected 'parent config' error, got: {text}"
    );
}

#[tokio::test]
async fn test_subagent_empty_prompt_returns_error() {
    // Verify that an empty prompt returns an error.
    let manager = Arc::new(SubagentManager::new());
    let (config, _temp) = make_minimal_run_config();
    manager.set_parent_config(config);
    let tool = AgentTool::new(manager, true);

    let mut input = make_tool_input("");
    input.input.as_object_mut().unwrap().insert(
        "prompt".to_string(),
        serde_json::Value::String(String::new()),
    );

    let output = tool.call(input).await;

    assert!(!output.is_text(), "expected error output");
    let text = output
        .content
        .first()
        .and_then(|p| match p {
            synthia_provider::types::ContentPart::Text(t) => {
                Some(t.text.clone())
            }
            _ => None,
        })
        .unwrap_or_default();
    assert!(
        text.contains("description and prompt parameters are required"),
        "expected prompt error, got: {text}"
    );
}
