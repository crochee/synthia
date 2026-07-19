//! `EventMsg` — server-to-client event envelope.

use serde::{Deserialize, Serialize};

use crate::id::{CallId, MessageId, SessionId, TurnId};

/// Server-to-client event envelope.
///
/// Every state change in the agent loop emits an `EventMsg` consumed by:
/// - CLI (stdout streaming)
/// - HTTP clients (SSE / WebSocket via `/ws`)
/// - IDE plugins (TUI/editor)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "msg", rename_all = "snake_case")]
#[non_exhaustive]
pub enum EventMsg {
    SessionCreated {
        session_id: SessionId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_session_id: Option<SessionId>,
        cli_version: String,
    },
    TurnStarted {
        session_id: SessionId,
        turn_id: TurnId,
        model: String,
    },
    TurnComplete {
        session_id: SessionId,
        turn_id: TurnId,
        status: TurnStatus,
    },
    ToolCall {
        session_id: SessionId,
        turn_id: TurnId,
        call_id: CallId,
        tool_name: String,
        args: serde_json::Value,
    },
    ToolCallOutput {
        session_id: SessionId,
        turn_id: TurnId,
        call_id: CallId,
        output: ToolOutput,
    },
    ApprovalRequest {
        session_id: SessionId,
        request: crate::approval::ApprovalRequest,
    },
    ApprovalResponded {
        session_id: SessionId,
        request_id: crate::id::ApprovalId,
        decision: crate::approval::PermissionDecision,
    },
    CompactStarted {
        session_id: SessionId,
        reason: CompactReason,
        current_tokens: u64,
        threshold: u64,
        can_cancel: bool,
    },
    CompactCompleted {
        session_id: SessionId,
        summary: String,
        dropped_message_ids: Vec<MessageId>,
        new_leaf: MessageId,
    },
    ThreadRolledBack {
        session_id: SessionId,
        target_message_id: MessageId,
        num_turns: u32,
    },
    TokenCount {
        session_id: SessionId,
        info: TokenUsage,
    },
    ModelRerouted {
        session_id: SessionId,
        from: String,
        to: String,
        reason: String,
    },
    ToolSearched {
        session_id: SessionId,
        query: String,
        results: Vec<String>,
    },
    Error {
        session_id: SessionId,
        kind: String,
        payload: serde_json::Value,
        recoverable: bool,
    },
    Warning {
        session_id: SessionId,
        message: String,
    },
    /// A custom event projected from an extension or plugin.
    ///
    /// Maps from `AgentEvent::Custom { event_type, data }` via the
    /// [`project_custom_event`] function. The `rendered` field contains
    /// the human-readable output from an `EventRenderer`; if rendering
    /// fails, it falls back to the raw JSON string.
    CustomEvent {
        session_id: SessionId,
        event_type: String,
        data: serde_json::Value,
        rendered: String,
    },
}

/// Turn completion status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Completed,
    Interrupted,
    Failed,
    Cancelled,
}

/// Tool call output envelope (success or error).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolOutput {
    Success { value: serde_json::Value },
    Error { message: String, interrupted: bool },
}

/// Reason for context compaction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompactReason {
    Manual,
    Auto,
    Hook,
}

/// Token usage snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_call_serde() {
        let msg = EventMsg::ToolCall {
            session_id: SessionId::new(),
            turn_id: TurnId::new(),
            call_id: CallId::new(),
            tool_name: "bash".to_string(),
            args: serde_json::json!({"cmd": "ls"}),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"msg\":\"tool_call\""));
        let parsed: EventMsg = serde_json::from_str(&json).unwrap();
        match parsed {
            EventMsg::ToolCall { tool_name, .. } => {
                assert_eq!(tool_name, "bash")
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn compact_completed_carries_summary() {
        let msg = EventMsg::CompactCompleted {
            session_id: SessionId::new(),
            summary: "user was editing config.toml".to_string(),
            dropped_message_ids: vec![],
            new_leaf: MessageId::new(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("config.toml"));
    }
}
