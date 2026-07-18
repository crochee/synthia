//! The [`Message`] struct — a single LLM message with content
//! and optional tool-call metadata.
//!
//! `tool_result_cleared_at` is a marker set by
//! `synthia_context::pruning::prune` when a tool-result message
//! has been pushed out of the protected tail. When `Some(_)`, the
//! message's original content MUST be replaced with a placeholder
//! by the LLM-context rendering layer
//! (`synthia_context::truncate::truncate_messages`), while the
//! in-memory `content` and on-disk event log retain the full
//! original bytes for replay / recovery (P8: transform, never lose).
//!
//! `#[serde(default)]` keeps pre-existing on-disk messages (which
//! lack the field) deserializable as `None` — the canonical default.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{content::Content, message_kind::MessageKind, role::Role};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: Role,
    pub content: Content,
    pub tool_call_id: Option<String>,
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_result_cleared_at: Option<DateTime<Utc>>,
}

impl Default for Message {
    fn default() -> Self {
        Self {
            role: Role::User,
            content: Content::text(""),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        }
    }
}

impl Message {
    pub fn new(role: Role, content: impl Into<Content>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_call_id: None,
            name: None,
            tool_result_cleared_at: None,
        }
    }

    pub fn user(text: impl Into<String>) -> Self {
        Self::new(Role::User, Content::text(text))
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self::new(Role::Assistant, Content::text(text))
    }

    pub fn system(text: impl Into<String>) -> Self {
        Self::new(Role::System, Content::text(text))
    }

    pub fn tool(
        content: impl Into<Content>,
        tool_call_id: impl Into<String>,
    ) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            name: None,
            tool_result_cleared_at: None,
        }
    }

    /// Classify this message into a [`MessageKind`].
    ///
    /// This is **O(1)** and **side-effect-free**: it examines only the
    /// `role` field and (for `Role::Assistant`) whether the content
    /// contains tool-use requests.
    pub fn kind(&self) -> MessageKind {
        let has_tool_calls = self.content.has_tool_use();
        MessageKind::from_role(self.role, has_tool_calls)
    }

    /// Whether this message should be included in the LLM's context
    /// window.
    ///
    /// # Contract
    ///
    /// - **O(1)** time complexity — no allocation, no I/O.
    /// - **Side-effect-free** — purely determined by `role` and content
    ///   emptiness.
    /// - **Deterministic** — same input always produces the same output.
    ///
    /// # Rules
    ///
    /// | `MessageKind` | `llm_visible()` |
    /// |---------------|-----------------|
    /// | `System`      | `true`          |
    /// | `User`        | `true`          |
    /// | `Assistant`   | `true`          |
    /// | `ToolCall`    | `true`          |
    /// | `ToolResult`  | `true` if content is non-empty; `false` if empty |
    pub fn llm_visible(&self) -> bool {
        match self.kind() {
            MessageKind::System
            | MessageKind::User
            | MessageKind::Assistant
            | MessageKind::ToolCall => true,
            MessageKind::ToolResult => !self.is_content_empty(),
        }
    }

    /// Check if the message content is effectively empty.
    ///
    /// A message is considered empty if it has no text parts and no
    /// tool-use parts. This handles both `Content::Single` and
    /// `Content::Multi` variants.
    fn is_content_empty(&self) -> bool {
        match &self.content {
            Content::Single(part) => match part {
                super::ContentPart::Text(tc) => tc.text.is_empty(),
                super::ContentPart::ToolUse(_) => false,
                super::ContentPart::ToolResult(_) => false,
                _ => true,
            },
            Content::Multi(parts) => parts.iter().all(|part| match part {
                super::ContentPart::Text(tc) => tc.text.is_empty(),
                super::ContentPart::ToolUse(_) => false,
                super::ContentPart::ToolResult(_) => false,
                _ => true,
            }),
        }
    }
}
