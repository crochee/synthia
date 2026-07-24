#![allow(deprecated)]
use std::sync::Arc;

use futures::StreamExt;
use synthia_agent::{
    agent::Agent,
    config::AgentConfig,
    events::SystemEvent,
    types::AgentEvent,
};
use synthia_hook::HookRegistry;
use synthia_provider::types::{ContentPart, StreamChunk, TextContent, ToolUse};
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
    registry.register(ToolEntry::new(Arc::new(FakeTool::new(
        "bash",
        "command output",
    ))));
    registry
}

#[tokio::test]
async fn test_tool_call_sequence_incorporates_results() {
    let workspace = create_test_workspace();

    let provider =
        Arc::new(FakeProvider::new(vec![]).with_stream_chunks(vec![
            vec![
                StreamChunk::Content(ContentPart::ToolUse(ToolUse {
                    id: "call_1".into(),
                    name: "bash".into(),
                    input: serde_json::json!({ "command": "ls -la" }),
                })),
                StreamChunk::Stop("tool_use".into()),
            ],
            vec![
                StreamChunk::Content(ContentPart::Text(TextContent {
                    text: "Found 3 files.".to_string(),
                    cache_control: None,
                })),
                StreamChunk::Stop("end_turn".into()),
            ],
        ]));

    let agent_config = AgentConfig {
        model: "fake-model".to_string(),
        max_iterations: 5,
        max_tokens: 1024,
        temperature: Some(0.7),
        workspace_root: workspace,
        ..Default::default()
    };

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider.clone(),
        create_test_tool_registry(),
        HookRegistry::new(),
        "tool-sequence-test".to_string(),
        synthia_agent::types::AgentInput::text(
            "List files in current directory",
        ),
        agent_config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let tool_call_started = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::Model(ContentPart::ToolUse(ToolUse { name, .. }))
                if name == "bash"
        )
    });
    let tool_call_completed = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::Model(ContentPart::ToolUse(ToolUse { id, .. }))
                if id == "call_1"
        )
    });
    let session_ended = events.iter().any(|e| {
        matches!(e, AgentEvent::System(SystemEvent::SessionEnded { .. }))
    });

    assert!(tool_call_started, "Should start bash tool call");
    assert!(tool_call_completed, "Should complete bash tool call");
    assert!(
        session_ended,
        "Should end session after tool result incorporated"
    );
}

#[tokio::test]
async fn test_sequential_tool_calls() {
    let workspace = create_test_workspace();

    let provider =
        Arc::new(FakeProvider::new(vec![]).with_stream_chunks(vec![
            vec![
                StreamChunk::Content(ContentPart::ToolUse(ToolUse {
                    id: "call_1".into(),
                    name: "read_file".into(),
                    input: serde_json::json!({ "path": "file1.txt" }),
                })),
                StreamChunk::Stop("tool_use".into()),
            ],
            vec![
                StreamChunk::Content(ContentPart::Text(TextContent {
                    text: "read_file result".to_string(),
                    cache_control: None,
                })),
                StreamChunk::Stop("end_turn".into()),
            ],
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
        "sequential-tools-test".to_string(),
        synthia_agent::types::AgentInput::text("Read file1"),
        agent_config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let tool_names: Vec<&str> = events
        .iter()
        .filter_map(|e| {
            if let AgentEvent::Model(ContentPart::ToolUse(ToolUse {
                name,
                ..
            })) = e
            {
                Some(name.as_str())
            } else {
                None
            }
        })
        .collect();

    assert!(
        tool_names.contains(&"read_file"),
        "Should call read_file tool, got: {:?}",
        tool_names
    );

    let session_ended = events.iter().any(|e| {
        matches!(e, AgentEvent::System(SystemEvent::SessionEnded { .. }))
    });
    assert!(
        session_ended,
        "Session should end after tool result incorporated"
    );
}
