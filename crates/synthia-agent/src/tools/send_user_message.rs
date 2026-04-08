//! SendUserMessage tool implementation
//!
//! Send a message to the user during agent execution.

use async_trait::async_trait;
use rmcp::model::CallToolResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::Tool;

/// Message status
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MessageStatus {
    #[default]
    Normal,
    Proactive,
}

/// SendUserMessage input parameters
#[derive(Debug, Clone, Deserialize)]
struct SendUserMessageParams {
    message: String,
    #[serde(default)]
    attachments: Vec<String>,
    #[serde(default)]
    status: MessageStatus,
}

/// Tool for sending messages to the user.
///
/// This tool allows the agent to proactively communicate with the user
/// by sending messages that will be displayed in the user interface.
#[derive(Debug, Clone)]
pub struct SendUserMessageTool {
    _private: (),
}

impl SendUserMessageTool {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for SendUserMessageTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for SendUserMessageTool {
    fn name(&self) -> &str {
        "SendUserMessage"
    }

    fn description(&self) -> &str {
        "Send a message to the user. Use this tool to proactively communicate with the user about progress, results, or important information."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "The message to send to the user"
                },
                "attachments": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "Optional list of attachment paths or URLs to include with the message",
                    "default": []
                },
                "status": {
                    "type": "string",
                    "enum": ["normal", "proactive"],
                    "description": "Message status: 'normal' for standard messages, 'proactive' for autonomous updates",
                    "default": "normal"
                }
            },
            "required": ["message"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let params: SendUserMessageParams = match serde_json::from_value(args) {
            Ok(p) => p,
            Err(e) => {
                return CallToolResult::error(vec![
                    rmcp::model::Content::text(format!(
                        "Invalid arguments: {e}"
                    )),
                ]);
            }
        };

        let status_str = match params.status {
            MessageStatus::Normal => "normal",
            MessageStatus::Proactive => "proactive",
        };

        let mut output = serde_json::json!({
            "status": status_str,
            "message": params.message,
        });

        if !params.attachments.is_empty() {
            output["attachments"] = serde_json::json!(params.attachments);
        }

        CallToolResult::success(vec![rmcp::model::Content::text(
            output.to_string(),
        )])
    }

    fn is_read_only(&self, _args: &Value) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_user_message_params_deserialization() {
        let json = serde_json::json!({
            "message": "Hello, user!"
        });
        let params: SendUserMessageParams =
            serde_json::from_value(json).unwrap();
        assert_eq!(params.message, "Hello, user!");
        assert!(params.attachments.is_empty());
        assert!(matches!(params.status, MessageStatus::Normal));
    }

    #[test]
    fn test_send_user_message_params_with_attachments() {
        let json = serde_json::json!({
            "message": "Here are your results",
            "attachments": ["/path/to/file1.txt", "/path/to/file2.txt"]
        });
        let params: SendUserMessageParams =
            serde_json::from_value(json).unwrap();
        assert_eq!(params.message, "Here are your results");
        assert_eq!(params.attachments.len(), 2);
    }

    #[test]
    fn test_send_user_message_params_proactive_status() {
        let json = serde_json::json!({
            "message": "Working on it",
            "status": "proactive"
        });
        let params: SendUserMessageParams =
            serde_json::from_value(json).unwrap();
        assert!(matches!(params.status, MessageStatus::Proactive));
    }

    #[tokio::test]
    async fn test_send_user_message_tool_call() {
        let tool = SendUserMessageTool::new();
        let args = serde_json::json!({
            "message": "Test message"
        });

        let result = tool.call(args).await;
        assert!(result.is_error != Some(true));
    }

    #[tokio::test]
    async fn test_send_user_message_tool_invalid_args() {
        let tool = SendUserMessageTool::new();
        let args = serde_json::json!({
            "invalid": "args"
        });

        let result = tool.call(args).await;
        assert!(result.is_error == Some(true));
    }

    #[tokio::test]
    async fn test_send_user_message_tool_is_read_only() {
        let tool = SendUserMessageTool::new();
        assert!(tool.is_read_only(&serde_json::Value::Null));
    }

    #[test]
    fn test_message_status_default() {
        let status = MessageStatus::default();
        assert!(matches!(status, MessageStatus::Normal));
    }
}
