//! SendMessage / SendMessageStream Tool — A2A 通信工具。
//!
//! LLM 通过这两个 Tool 与远程 agent 交互：
//! - SendMessage: A2A 同步通信，等 Task 完成
//! - SendMessageStream: A2A 流式通信，实时接收输出

use std::sync::Arc;

use async_trait::async_trait;
use synthia_tool::{
    traits::{ExecutionMode, Tool},
    types::{ToolInput, ToolOutput},
};

use crate::transport::A2aTransport;

/// SendMessage Tool — A2A 同步通信。
///
/// 向远程 agent 发消息，等 Task 完成后返回结果。
pub struct SendMessageTool {
    /// 目标 agent URL。
    target_url: String,
    /// A2A 通信层（Phase 2 用于实际 A2A client 调用）。
    #[allow(dead_code)]
    // reason: Phase 2 stub — will be used by A2A client calls
    transport: Arc<A2aTransport>,
}

impl SendMessageTool {
    /// 创建指向指定 URL 的 SendMessageTool。
    pub fn for_url(target_url: String, transport: Arc<A2aTransport>) -> Self {
        Self {
            target_url,
            transport,
        }
    }

    /// 获取目标 URL。
    pub fn target_url(&self) -> &str {
        &self.target_url
    }
}

#[async_trait]
impl Tool for SendMessageTool {
    fn name(&self) -> &str {
        "SendMessage"
    }

    fn description(&self) -> &str {
        "Send a message to a remote agent via A2A protocol and wait for response"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Message to send to the remote agent"
                },
                "metadata": {
                    "type": "object",
                    "description": "Optional metadata for the message"
                }
            },
            "required": ["message"]
        })
    }

    fn requires_permission(&self) -> bool {
        false
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Parallel
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let message = input
            .input
            .as_object()
            .and_then(|obj| obj.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if message.is_empty() {
            return ToolOutput::error("message parameter is required");
        }

        // TODO: Phase 2 — 实际 A2A client 调用
        // client.send_message(SendMessageRequest { message, .. }) → Task → Artifact
        // 当前返回占位结果
        tracing::info!(
            target_url = %self.target_url,
            message_len = message.len(),
            "SendMessageTool: sending message via A2A"
        );

        ToolOutput::text(format!(
            "[SendMessage] → {url}: {message}",
            url = self.target_url,
        ))
    }
}

/// SendMessageStream Tool — A2A 流式通信。
///
/// 向远程 agent 发消息，流式接收输出。
pub struct SendMessageStreamTool {
    /// 目标 agent URL。
    target_url: String,
    /// A2A 通信层（Phase 2 用于实际 A2A streaming client 调用）。
    #[allow(dead_code)]
    // reason: Phase 2 stub — will be used by A2A streaming client calls
    transport: Arc<A2aTransport>,
}

impl SendMessageStreamTool {
    /// 创建指向指定 URL 的 SendMessageStreamTool。
    pub fn for_url(target_url: String, transport: Arc<A2aTransport>) -> Self {
        Self {
            target_url,
            transport,
        }
    }

    /// 获取目标 URL。
    pub fn target_url(&self) -> &str {
        &self.target_url
    }
}

#[async_trait]
impl Tool for SendMessageStreamTool {
    fn name(&self) -> &str {
        "SendMessageStream"
    }

    fn description(&self) -> &str {
        "Send a message to a remote agent via A2A and receive streaming response"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Message to send to the remote agent"
                },
                "metadata": {
                    "type": "object",
                    "description": "Optional metadata for the message"
                }
            },
            "required": ["message"]
        })
    }

    fn requires_permission(&self) -> bool {
        false
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Parallel
    }

    async fn call(&self, input: ToolInput) -> ToolOutput {
        let message = input
            .input
            .as_object()
            .and_then(|obj| obj.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if message.is_empty() {
            return ToolOutput::error("message parameter is required");
        }

        // TODO: Phase 2 — 实际 A2A streaming client 调用
        // client.send_streaming_message() → Stream<Event> → collect
        tracing::info!(
            target_url = %self.target_url,
            message_len = message.len(),
            "SendMessageStreamTool: sending streaming message via A2A"
        );

        ToolOutput::text(format!(
            "[SendMessageStream] → {url}: {message}",
            url = self.target_url,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::transport::AgentSkill;

    fn make_transport() -> Arc<A2aTransport> {
        Arc::new(A2aTransport::from_handle_info(
            "test".to_string(),
            "test agent".to_string(),
            vec![AgentSkill {
                id: "test".to_string(),
                name: "test".to_string(),
                description: "test skill".to_string(),
            }],
        ))
    }

    fn make_tool_input(name: &str, message: &str) -> ToolInput {
        ToolInput {
            name: name.to_string(),
            input: serde_json::json!({ "message": message }),
            context: synthia_tool::types::ToolExecutionContext::new(
                String::new(),
                PathBuf::new(),
            ),
        }
    }

    #[tokio::test]
    async fn send_message_tool_call() {
        let transport = make_transport();
        let tool = SendMessageTool::for_url(
            "http://localhost:8080".to_string(),
            transport,
        );
        assert_eq!(tool.name(), "SendMessage");
        assert_eq!(tool.target_url(), "http://localhost:8080");

        let input = make_tool_input("SendMessage", "hello");
        let output = tool.call(input).await;
        assert!(output.is_text());
    }

    #[tokio::test]
    async fn send_message_stream_tool_call() {
        let transport = make_transport();
        let tool = SendMessageStreamTool::for_url(
            "http://localhost:8080".to_string(),
            transport,
        );
        assert_eq!(tool.name(), "SendMessageStream");

        let input = make_tool_input("SendMessageStream", "hello");
        let output = tool.call(input).await;
        assert!(output.is_text());
    }

    #[tokio::test]
    async fn send_message_empty_message() {
        let transport = make_transport();
        let tool = SendMessageTool::for_url(
            "http://localhost:8080".to_string(),
            transport,
        );
        let input = make_tool_input("SendMessage", "");
        let output = tool.call(input).await;
        assert!(!output.is_text());
    }
}
