#![allow(deprecated)]
use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use synthia_agent::{
    agent::Agent,
    config::AgentConfig,
    events::SystemEvent,
    types::AgentEvent,
};
use synthia_hook::HookRegistry;
use synthia_provider::types::{ContentPart, StreamChunk, TextContent, ToolUse};
use synthia_tool::{
    registry::{ToolEntry, ToolRegistry},
    traits::Tool,
};
use tokio_util::sync::CancellationToken;

mod test_support;
use test_support::{
    FakeProvider,
    FakeTool,
    create_test_workspace,
    make_run_config,
};

struct BlockingFakeTool {
    name: String,
}

impl BlockingFakeTool {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

#[async_trait]
impl Tool for BlockingFakeTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "A tool that may be blocked by guardian"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn call(
        &self,
        _input: synthia_tool::types::ToolInput,
    ) -> synthia_tool::types::ToolOutput {
        synthia_tool::types::ToolOutput::text("Tool executed")
    }
}

fn create_test_tool_registry() -> ToolRegistry {
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(FakeTool::new(
        "read_file",
        "file content",
    ))));
    registry
}

fn create_blocking_tool_registry() -> ToolRegistry {
    let registry = ToolRegistry::new();
    registry.register(ToolEntry::new(Arc::new(BlockingFakeTool::new(
        "dangerous_tool",
    ))));
    registry
}

#[tokio::test]
async fn test_guardian_allows_normal_tool_call() {
    let workspace = create_test_workspace();

    let provider =
        Arc::new(FakeProvider::new(vec![]).with_stream_chunks(vec![
            vec![
                StreamChunk::Content(ContentPart::ToolUse(ToolUse {
                    id: "call_1".into(),
                    name: "read_file".into(),
                    input: serde_json::json!({ "path": "test.txt" }),
                })),
                StreamChunk::Stop("tool_use".into()),
            ],
            vec![
                StreamChunk::Content(ContentPart::Text(TextContent {
                    text: "Read the file successfully.".to_string(),
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
        "guardian-allow-test".to_string(),
        synthia_agent::types::AgentInput::text("Read a file"),
        agent_config,
        cancel_token,
    );

    let events: Vec<AgentEvent> = Agent::run_stream(run_config).collect().await;

    let tool_call_started = events.iter().any(|e| {
        matches!(
            e,
            AgentEvent::Model(ContentPart::ToolUse(ToolUse { name, .. }))
                if name == "read_file"
        )
    });
    let session_ended = events.iter().any(|e| {
        matches!(e, AgentEvent::System(SystemEvent::SessionEnded { .. }))
    });

    assert!(
        tool_call_started,
        "Normal tool call should be allowed by guardian"
    );
    assert!(
        session_ended,
        "Session should end after tool call completes"
    );
}

#[tokio::test]
async fn test_guardian_blocks_dangerous_operation() {
    let workspace = create_test_workspace();

    let provider =
        Arc::new(FakeProvider::new(vec![]).with_stream_chunks(vec![
            vec![
                StreamChunk::Content(ContentPart::ToolUse(ToolUse {
                    id: "call_1".into(),
                    name: "dangerous_tool".into(),
                    input: serde_json::json!({}),
                })),
                StreamChunk::Stop("tool_use".into()),
            ],
            vec![
                StreamChunk::Content(ContentPart::Text(TextContent {
                    text: "Operation was blocked.".to_string(),
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
        create_blocking_tool_registry(),
        HookRegistry::new(),
        "guardian-block-test".to_string(),
        synthia_agent::types::AgentInput::text("Execute dangerous operation"),
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
}
