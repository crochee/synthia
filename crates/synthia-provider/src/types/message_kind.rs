//! `MessageKind` enum — a 5-variant classification of LLM messages.
//!
//! Extends the 4-variant [`Role`] with a `ToolCall` variant that
//! distinguishes assistant messages containing tool invocations from
//! plain assistant text. This enables [`Message::llm_visible()`] to
//! make fine-grained visibility decisions without inspecting content.

use serde::{Deserialize, Serialize};

use super::role::Role;

/// Fine-grained message classification for LLM visibility decisions.
///
/// Unlike [`Role`] (which has 4 variants and maps to provider wire
/// format), `MessageKind` has 5 variants and is used purely for
/// in-process routing — deciding which messages to include in the
/// LLM's context window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    System,
    User,
    Assistant,
    /// An assistant message that contains tool-use requests.
    ToolCall,
    /// A tool result message (role = Tool).
    ToolResult,
}

impl MessageKind {
    /// Map a [`Role`] + "has tool calls" flag to a [`MessageKind`].
    ///
    /// This is O(1) and side-effect-free.
    pub fn from_role(role: Role, has_tool_calls: bool) -> Self {
        match role {
            Role::System => Self::System,
            Role::User => Self::User,
            Role::Assistant if has_tool_calls => Self::ToolCall,
            Role::Assistant => Self::Assistant,
            Role::Tool => Self::ToolResult,
        }
    }
}
