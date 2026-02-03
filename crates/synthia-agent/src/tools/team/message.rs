use async_trait::async_trait;
use rmcp::model::CallToolResult;
use serde_json::Value;

use super::{
    file_store::TeamStorage,
    shared::err_result,
    tool_base::{json_result, text_result},
    types::{
        BroadcastRequest,
        MessageType as MsgType,
        SendMessageRequest,
        TeamMessage,
    },
};
use crate::tools::Tool;

#[derive(Clone)]
pub(crate) struct SendMessageTool {
    storage: TeamStorage,
}

impl SendMessageTool {
    pub(crate) fn new() -> Self {
        Self {
            storage: TeamStorage::new(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn new_with_storage(storage: TeamStorage) -> Self {
        Self { storage }
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
        "send_message"
    }

    fn description(&self) -> &str {
        "Send message to a teammate."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schemars::schema_for!(SendMessageRequest))
            .unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: SendMessageRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => return err_result(format!("Invalid request: {e}")),
        };

        let msg_type = request.msg_type.unwrap_or(MsgType::Message);

        if let Err(e) = self
            .storage
            .message_store
            .send_message(&request.to, msg_type, "lead", &request.content, None)
            .await
        {
            return err_result(format!("Failed to send message: {e}"));
        }

        text_result(format!("Sent {} to {}", msg_type.as_str(), request.to))
    }
}

#[derive(Clone)]
pub(crate) struct ReadInboxTool {
    storage: TeamStorage,
}

impl ReadInboxTool {
    pub(crate) fn new() -> Self {
        Self {
            storage: TeamStorage::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_storage(storage: TeamStorage) -> Self {
        Self { storage }
    }
}

impl Default for ReadInboxTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ReadInboxTool {
    fn name(&self) -> &str {
        "read_inbox"
    }

    fn description(&self) -> &str {
        "Read and clear inbox messages. Messages are marked as read atomically."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn call(&self, _args: Value) -> CallToolResult {
        let messages = match self.storage.message_store.read_inbox("lead").await
        {
            Ok(m) => m,
            Err(e) => return err_result(format!("Failed to read inbox: {e}")),
        };

        let tool_messages: Vec<TeamMessage> = messages
            .into_iter()
            .map(|m| TeamMessage {
                msg_type: m.msg_type,
                from: m.sender,
                content: m.content,
                timestamp: m.timestamp,
                request_id: m.request_id,
            })
            .collect();

        if let Err(e) =
            self.storage.message_store.mark_messages_read("lead").await
        {
            return err_result(format!("Failed to mark messages as read: {e}"));
        }

        json_result(&tool_messages)
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

    #[cfg(test)]
    pub(crate) fn new_with_storage(storage: TeamStorage) -> Self {
        Self { storage }
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
        "Broadcast message to all teammates."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schemars::schema_for!(BroadcastRequest))
            .unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: BroadcastRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => return err_result(format!("Invalid request: {e}")),
        };

        let count = match self
            .storage
            .message_store
            .broadcast("lead", &request.content, &self.storage.teammate_store)
            .await
        {
            Ok(c) => c,
            Err(e) => return err_result(format!("Failed to broadcast: {e}")),
        };

        text_result(format!("Broadcast to {count} teammates"))
    }
}
