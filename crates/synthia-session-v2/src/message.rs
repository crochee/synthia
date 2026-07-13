//! `Message` and `MessageInfo` — opencode V2 message model.
//!
//! `Message` = `{ info: MessageInfo, parts: Vec<Part> }`
//! (mirrors opencode `WithParts` in `packages/opencode/src/session/message-v2.ts:206-413`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use synthia_protocol::{MessageId, W3cTraceContext};

use crate::part::Part;

/// Role of a message in a conversation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

/// Message envelope. Carries `info` (metadata) + ordered `parts` (content).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub info: MessageInfo,
    pub parts: Vec<Part>,
}

/// Message metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageInfo {
    pub id: MessageId,
    /// Parent in the append-only tree (None for root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_message_id: Option<MessageId>,
    pub role: Role,
    pub time: MessageTime,
    /// Agent that produced this (None for user messages).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    /// Model that produced this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// W3C trace context for this message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<W3cTraceContext>,
    /// True if this message is itself a compaction summary (not original content).
    pub summary: bool,
    /// Error attached to this message (if it failed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<MessageError>,
}

/// Message timing.
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default,
)]
pub struct MessageTime {
    pub created: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed: Option<DateTime<Utc>>,
}

/// Message-level error.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageError {
    pub kind: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_message_serde() {
        let msg = Message {
            info: MessageInfo {
                id: MessageId::new(),
                parent_message_id: None,
                role: Role::User,
                time: MessageTime {
                    created: Utc::now(),
                    completed: None,
                },
                agent_name: None,
                model_id: None,
                trace: None,
                summary: false,
                error: None,
            },
            parts: vec![],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.info.role, Role::User);
    }
}
