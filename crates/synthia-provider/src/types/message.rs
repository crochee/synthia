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

use super::{content::Content, role::Role};

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
}
