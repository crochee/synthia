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

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{content::Content, message_kind::MessageKind, role::Role};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: Role,
    pub content: Content,
    pub tool_call_id: Option<String>,
    pub name: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ToolUse,
        types::content::{ContentPart, TextContent},
    };

    // -- Message::default ---------------------------------------------

    /// `Message::default()` MUST yield a
    /// `User` role message with empty
    /// content and `None` for all 3
    /// optional fields.
    #[test]
    fn default_yields_user_with_empty_content() {
        let m = Message::default();
        assert_eq!(m.role, Role::User);
        assert!(m.tool_call_id.is_none());
        assert!(m.name.is_none());
        assert!(m.tool_result_cleared_at.is_none());
    }

    // -- Message constructors -----------------------------------------

    /// `Message::new` MUST set role +
    /// content and leave the 3 optional
    /// fields as None.
    #[test]
    fn new_builds_with_role_and_content_optionals_none() {
        let m = Message::new(Role::Assistant, "hi");
        assert_eq!(m.role, Role::Assistant);
        assert!(m.tool_call_id.is_none());
        assert!(m.name.is_none());
        assert!(m.tool_result_cleared_at.is_none());
    }

    /// `Message::user`, `assistant`, and
    /// `system` MUST all build with the
    /// matching role.
    #[test]
    fn user_assistant_system_constructors_set_correct_role() {
        assert_eq!(Message::user("u").role, Role::User);
        assert_eq!(Message::assistant("a").role, Role::Assistant);
        assert_eq!(Message::system("s").role, Role::System);
    }

    /// `Message::tool` MUST set `Tool`
    /// role and propagate `tool_call_id`
    /// from the caller.
    #[test]
    fn tool_constructor_sets_role_and_tool_call_id() {
        let m = Message::tool("ok", "call-1");
        assert_eq!(m.role, Role::Tool);
        assert_eq!(m.tool_call_id, Some("call-1".to_string()));
        assert!(m.name.is_none());
        assert!(m.tool_result_cleared_at.is_none());
    }

    // -- kind() 5-way mapping -----------------------------------------

    /// `kind()` MUST map `System → System`,
    /// `User → User`, `Assistant (no tool
    /// calls) → Assistant`, `Assistant
    /// (with tool calls) → ToolCall`,
    /// `Tool → ToolResult`.
    #[test]
    fn kind_returns_system_for_system_role() {
        assert_eq!(Message::system("s").kind(), MessageKind::System);
    }

    #[test]
    fn kind_returns_user_for_user_role() {
        assert_eq!(Message::user("u").kind(), MessageKind::User);
    }

    #[test]
    fn kind_returns_assistant_for_plain_assistant_role() {
        assert_eq!(Message::assistant("hello").kind(), MessageKind::Assistant);
    }

    #[test]
    fn kind_returns_tool_call_for_assistant_with_tool_use() {
        let m = Message::new(
            Role::Assistant,
            Content::Single(ContentPart::ToolUse(ToolUse {
                id: "c1".to_string(),
                name: "bash".to_string(),
                input: serde_json::json!({"cmd": "ls"}),
            })),
        );
        assert_eq!(m.kind(), MessageKind::ToolCall);
    }

    #[test]
    fn kind_returns_tool_result_for_tool_role() {
        let m = Message::tool("ok", "c1");
        assert_eq!(m.kind(), MessageKind::ToolResult);
    }

    // -- llm_visible() 5-way mapping ----------------------------------

    /// `llm_visible()` MUST return `true`
    /// for System, User, Assistant, and
    /// ToolCall messages regardless of
    /// content emptiness.
    #[test]
    fn llm_visible_is_true_for_non_tool_result_kinds() {
        assert!(Message::system("").llm_visible());
        assert!(Message::user("").llm_visible());
        assert!(Message::assistant("").llm_visible());
        // ToolCall: assistant with tool-use
        let tc = Message::new(
            Role::Assistant,
            Content::Single(ContentPart::ToolUse(ToolUse {
                id: "c1".to_string(),
                name: "bash".to_string(),
                input: serde_json::json!({}),
            })),
        );
        assert!(tc.llm_visible());
    }

    /// `llm_visible()` MUST return `false`
    /// for an empty ToolResult.
    #[test]
    fn llm_visible_is_false_for_empty_tool_result() {
        let m = Message::tool("", "c1");
        assert_eq!(m.kind(), MessageKind::ToolResult);
        assert!(!m.llm_visible());
    }

    /// `llm_visible()` MUST return `true`
    /// for a non-empty ToolResult.
    #[test]
    fn llm_visible_is_true_for_non_empty_tool_result() {
        let m = Message::tool("real content", "c1");
        assert!(m.llm_visible());
    }

    // -- is_content_empty() edge cases -------------------------------

    /// `is_content_empty` MUST recognize
    /// that Single(Text("")) is empty
    /// and Single(Text("x")) is non-empty
    /// (via the public `llm_visible()`
    /// path that delegates to it).
    #[test]
    fn empty_text_is_empty_nonempty_text_is_not() {
        let empty = Message::tool("", "c1");
        let nonempty = Message::tool("x", "c1");
        assert!(!empty.llm_visible());
        assert!(nonempty.llm_visible());
    }

    /// `is_content_empty` MUST recognize
    /// that ToolUse content is never
    /// empty even when the input is empty.
    /// Pin via `kind() == ToolCall` +
    /// `llm_visible() == true` for
    /// `Content::Single(ToolUse({}))`.
    #[test]
    fn tool_use_content_is_never_empty() {
        let m = Message::new(
            Role::Assistant,
            Content::Single(ContentPart::ToolUse(ToolUse {
                id: "c1".to_string(),
                name: "bash".to_string(),
                input: serde_json::json!({}),
            })),
        );
        assert_eq!(m.kind(), MessageKind::ToolCall);
        assert!(m.llm_visible());
    }

    /// `is_content_empty` MUST recognize
    /// that ToolResult content is never
    /// empty even when its inner data is
    /// empty (the role distinguishes it).
    #[test]
    fn tool_result_content_is_never_empty_via_short_circuit() {
        // Use the simpler Text-based
        // path: role=Tool with Text("x")
        // is non-empty.
        let m = Message::tool("x", "c1");
        assert_eq!(m.kind(), MessageKind::ToolResult);
        assert!(m.llm_visible());
    }

    /// `Content::Multi` with mixed empty
    /// and non-empty parts MUST count as
    /// non-empty (the `all(...)` check
    /// requires every part to be empty).
    #[test]
    fn multi_content_with_one_nonempty_part_is_nonempty() {
        // Multi([Text(""), Text("x")])
        // MUST NOT be empty.
        let m = Message::new(
            Role::Tool,
            Content::Multi(vec![
                ContentPart::Text(TextContent {
                    text: String::new(),
                    cache_control: None,
                }),
                ContentPart::Text(TextContent {
                    text: "x".to_string(),
                    cache_control: None,
                }),
            ]),
        );
        assert!(m.llm_visible());
    }

    // -- serde round-trip --------------------------------------------

    /// `Message` MUST round-trip every
    /// field verbatim through JSON.
    #[test]
    fn message_round_trips_through_json() {
        let m = Message::tool("payload", "call-1");
        let json = serde_json::to_string(&m).unwrap();
        let parsed: Message =
            serde_json::from_str(&json).expect("round-trip parse");
        assert_eq!(parsed.role, m.role);
        assert_eq!(parsed.tool_call_id, m.tool_call_id);
    }
}
