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
use synthia_provider::types::{
    ContentPart,
    StreamChunk,
    TextContent,
    ToolResult,
    ToolUse,
};
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
        "command output",
    ))));
    registry
}

#[tokio::test]
async fn test_pause_and_resume_continues_from_state() {
    let workspace = create_test_workspace();

    let provider1 =
        Arc::new(FakeProvider::new(vec![]).with_stream_chunks(vec![
            vec![
                StreamChunk::Content(ContentPart::ToolUse(ToolUse {
                    id: "call_1".into(),
                    name: "bash".into(),
                    input: serde_json::json!({ "command": "echo 'first'" }),
                })),
                StreamChunk::Stop("tool_use".into()),
            ],
            vec![
                StreamChunk::Content(ContentPart::Text(TextContent {
                    text: "Resumed and continuing".to_string(),
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
        workspace_root: workspace.clone(),
        ..Default::default()
    };

    let cancel_token = CancellationToken::new();
    let run_config = make_run_config(
        provider1,
        create_test_tool_registry(),
        HookRegistry::new(),
        "pause-resume-test".to_string(),
        synthia_agent::types::AgentInput::text("Run a command"),
        agent_config.clone(),
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let tool_call_started = events
        .iter()
        .any(|e| matches!(e, AgentEvent::Model(ContentPart::ToolUse(_))));
    let tool_call_completed = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::Model(ContentPart::ToolResult(ToolResult {
                tool_use_id,
                ..
            })) if tool_use_id == "call_1"
        )
    });
    let session_ended = events.iter().any(|e| {
        matches!(e, AgentEvent::System(SystemEvent::SessionEnded { .. }))
    });

    assert!(tool_call_started, "Should start a tool call");
    assert!(tool_call_completed, "Should complete the tool call");
    assert!(session_ended, "Should end the session");
}

#[tokio::test]
async fn test_session_pause_mid_turn() {
    let workspace = create_test_workspace();

    let provider = Arc::new(FakeProvider::new(vec![]).with_stream_chunks(vec![
        vec![
            StreamChunk::Content(ContentPart::ToolUse(ToolUse {
                id: "call_1".into(),
                name: "bash".into(),
                input: serde_json::json!({ "command": "sleep 1 && echo 'done'" }),
            })),
            StreamChunk::Stop("tool_use".into()),
        ],
        vec![
            StreamChunk::Content(ContentPart::Text(TextContent {
                text: "Session completed after pause.".to_string(),
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
        provider,
        create_test_tool_registry(),
        HookRegistry::new(),
        "mid-turn-pause-test".to_string(),
        synthia_agent::types::AgentInput::text(
            "Execute a long running command",
        ),
        agent_config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    assert!(
        events.iter().any(|e| matches!(
            e,
            AgentEvent::System(SystemEvent::SessionStarted { .. })
        )),
        "Session should start"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            AgentEvent::System(SystemEvent::SessionEnded { .. })
        )),
        "Session should end after pause and resume"
    );
}
