use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde_json::Value;

use super::{
    file_store::TeamStorage,
    tool_base::json_result,
    types::{BroadcastRequest, MessageType as MsgType, SendMessageRequest},
};
use crate::{config::AgentName, tools::Tool};

/// Default sender name for Lead
const LEAD_SENDER: &str = "lead";

#[derive(Clone)]
pub(crate) struct SendMessageTool {
    storage: TeamStorage,
    parent_name: AgentName,
    agent_name: Option<String>,
}

impl SendMessageTool {
    pub(crate) fn new() -> Self {
        Self {
            storage: TeamStorage::new(),
            parent_name: AgentName::Solo,
            agent_name: None,
        }
    }

    pub(crate) fn with_parent_name(mut self, name: AgentName) -> Self {
        self.parent_name = name;
        self
    }

    pub(crate) fn with_agent_name(mut self, name: String) -> Self {
        self.agent_name = Some(name);
        self
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn new_with_storage_and_name(
        storage: TeamStorage,
        name: AgentName,
    ) -> Self {
        Self {
            storage,
            parent_name: name,
            agent_name: None,
        }
    }

    fn is_lead(&self) -> bool {
        self.parent_name.is_lead()
    }

    fn sender_name(&self) -> String {
        if self.is_lead() {
            LEAD_SENDER.to_string()
        } else {
            self.agent_name
                .clone()
                .unwrap_or_else(|| "unknown".to_string())
        }
    }
}

impl Default for SendMessageTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for SendMessageTool {
    fn name(&self) -> &str {
        "send_to_teammate"
    }

    fn description(&self) -> &str {
        "Send a message to another teammate. Available in Team mode."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schemars::schema_for!(SendMessageRequest))
            .unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: SendMessageRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid request: {e}"
                ))]);
            }
        };

        let sender = self.sender_name();
        let msg_type = request.msg_type.unwrap_or(MsgType::Message);

        match self
            .storage
            .message_store
            .send_message(
                &request.to,
                msg_type,
                &sender,
                &request.content,
                request.task_id.as_deref(),
            )
            .await
        {
            Ok(_) => CallToolResult::success(vec![Content::text(format!(
                "Message sent to {}",
                request.to
            ))]),
            Err(e) => CallToolResult::error(vec![Content::text(format!(
                "Failed to send message: {e}"
            ))]),
        }
    }
}

#[derive(Clone)]
pub(crate) struct ReceiveMessageTool {
    storage: TeamStorage,
    agent_name: Option<String>,
}

impl ReceiveMessageTool {
    pub(crate) fn new() -> Self {
        Self {
            storage: TeamStorage::new(),
            agent_name: None,
        }
    }

    pub(crate) fn with_agent_name(mut self, name: String) -> Self {
        self.agent_name = Some(name);
        self
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn new_with_agent_name(name: String) -> Self {
        Self {
            storage: TeamStorage::new(),
            agent_name: Some(name),
        }
    }
}

impl Default for ReceiveMessageTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ReceiveMessageTool {
    fn name(&self) -> &str {
        "read_inbox"
    }

    fn description(&self) -> &str {
        "Read messages from your inbox. Available in Team mode."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn call(&self, _args: Value) -> CallToolResult {
        let agent_name = match &self.agent_name {
            Some(name) => name.clone(),
            None => {
                return CallToolResult::error(vec![Content::text(
                    "Agent name not set",
                )]);
            }
        };

        match self.storage.message_store.read_inbox(&agent_name).await {
            Ok(messages) => json_result(&messages),
            Err(e) => CallToolResult::error(vec![Content::text(format!(
                "Failed to read inbox: {e}"
            ))]),
        }
    }
}

#[derive(Clone)]
pub(crate) struct BroadcastTool {
    storage: TeamStorage,
}

impl BroadcastTool {
    pub(crate) fn new() -> Self {
        Self {
            storage: TeamStorage::new(),
        }
    }
}

impl Default for BroadcastTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BroadcastTool {
    fn name(&self) -> &str {
        "broadcast"
    }

    fn description(&self) -> &str {
        "Broadcast a message to all teammates. Only available for Team Lead."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schemars::schema_for!(BroadcastRequest))
            .unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: BroadcastRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid request: {e}"
                ))]);
            }
        };

        match self
            .storage
            .message_store
            .broadcast(
                LEAD_SENDER,
                &request.content,
                &self.storage.teammate_store,
            )
            .await
        {
            Ok(_) => CallToolResult::success(vec![Content::text(
                "Message broadcast to all teammates",
            )]),
            Err(e) => CallToolResult::error(vec![Content::text(format!(
                "Failed to broadcast: {e}"
            ))]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_message_tool_name() {
        let tool = SendMessageTool::new();
        assert_eq!(tool.name(), "send_to_teammate");
    }

    #[test]
    fn test_receive_message_tool_name() {
        let tool = ReceiveMessageTool::new();
        assert_eq!(tool.name(), "read_inbox");
    }

    #[test]
    fn test_broadcast_tool_name() {
        let tool = BroadcastTool::new();
        assert_eq!(tool.name(), "broadcast");
    }

    #[test]
    fn test_send_message_tool_lead_sender() {
        let tool = SendMessageTool::new().with_parent_name(AgentName::Lead);
        assert_eq!(tool.sender_name(), "lead");
    }

    #[test]
    fn test_send_message_tool_member_sender() {
        let tool = SendMessageTool::new()
            .with_parent_name(AgentName::Custom("alice".to_string()))
            .with_agent_name("alice".to_string());
        assert_eq!(tool.sender_name(), "alice");
    }
}
