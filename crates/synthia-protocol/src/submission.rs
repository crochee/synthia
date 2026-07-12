//! `Submission` and `Op` enums — request envelope from clients to the agent loop.

use serde::{Deserialize, Serialize};

use crate::{
    id::{ApprovalId, CallId, MessageId, SessionId, SubmissionId},
    trace::W3cTraceContext,
};

/// Client-to-agent request envelope.
///
/// Every operation the user takes (type a message, interrupt, approve a tool,
/// compact, fork a session) becomes a `Submission` carrying an `Op`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Submission {
    pub id: SubmissionId,
    pub op: Op,
    /// Client-provided user message correlation ID (e.g., for UI ordering).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_user_message_id: Option<String>,
    /// W3C trace context for distributed tracing across CLI/server/IDE.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<W3cTraceContext>,
}

/// Operations the agent loop accepts from clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Op {
    /// Interrupt an in-flight turn.
    Interrupt {
        /// Human-readable reason (for telemetry + UI display).
        reason: String,
    },
    /// Trigger context compaction.
    Compact {
        /// Manual (user-requested) vs automatic (driven by token threshold).
        manual: bool,
        /// Optional hint to the compactor about what to preserve.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary_hint: Option<String>,
    },
    /// User input — text or attachments.
    UserInput {
        items: Vec<InputItem>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        final_output_json_schema: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        additional_context: Option<String>,
    },
    /// Roll back the session N turns (non-destructive — only the leaf pointer moves).
    ThreadRollback { num_turns: u32 },
    /// Respond to a pending approval request.
    ApprovalResponse {
        id: ApprovalId,
        decision: crate::approval::PermissionDecision,
    },
    /// Re-discover tools (after MCP server config changes, etc.).
    RefreshTools,
    /// Re-submit specific messages (re-run them with current model/tools).
    Resubmit { message_ids: Vec<MessageId> },
    /// Switch the active model.
    UpdateModel { model: String },
    /// Change the thinking/reasoning depth.
    UpdateThinkingLevel { level: ThinkingLevel },
    /// Switch to a different existing session.
    SwitchSession { session_id: SessionId },
    /// Fork the session at a specific message (creates new SessionId).
    ForkSession { at_message_id: MessageId },
}

/// Item types in `Op::UserInput`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum InputItem {
    Text {
        text: String,
    },
    Image {
        url: String,
    },
    File {
        path: String,
        content_b64: String,
    },
    Skill {
        name: String,
        args: serde_json::Value,
    },
    ToolCallOverride {
        call_id: CallId,
        output: serde_json::Value,
    },
}

/// Reasoning depth setting.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ThinkingLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_input_serde() {
        let op = Op::UserInput {
            items: vec![InputItem::Text {
                text: "hello".to_string(),
            }],
            final_output_json_schema: None,
            additional_context: None,
        };
        let json = serde_json::to_string(&op).unwrap();
        let parsed: Op = serde_json::from_str(&json).unwrap();
        match parsed {
            Op::UserInput { items, .. } => assert_eq!(items.len(), 1),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn interrupt_carries_reason() {
        let op = Op::Interrupt {
            reason: "user pressed Ctrl-C".to_string(),
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("\"op\":\"interrupt\""));
        assert!(json.contains("user pressed Ctrl-C"));
    }
}
