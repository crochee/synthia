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

#[cfg(test)]
mod tests {
    use super::*;

    // -- serde 5-way mapping -----------------------------------------

    /// `MessageKind` MUST serialize each
    /// variant in snake_case form
    /// (e.g. `"tool_call"`, not `"ToolCall"`).
    #[test]
    fn serializes_each_variant_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&MessageKind::System).unwrap(),
            "\"system\""
        );
        assert_eq!(
            serde_json::to_string(&MessageKind::User).unwrap(),
            "\"user\""
        );
        assert_eq!(
            serde_json::to_string(&MessageKind::Assistant).unwrap(),
            "\"assistant\""
        );
        assert_eq!(
            serde_json::to_string(&MessageKind::ToolCall).unwrap(),
            "\"tool_call\""
        );
        assert_eq!(
            serde_json::to_string(&MessageKind::ToolResult).unwrap(),
            "\"tool_result\""
        );
    }

    /// `MessageKind` MUST round-trip each
    /// variant through JSON.
    #[test]
    fn round_trips_each_variant_through_json() {
        for kind in [
            MessageKind::System,
            MessageKind::User,
            MessageKind::Assistant,
            MessageKind::ToolCall,
            MessageKind::ToolResult,
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            let parsed: MessageKind =
                serde_json::from_str(&json).expect("round-trip");
            assert_eq!(parsed, kind);
        }
    }

    /// `MessageKind` MUST reject unknown
    /// variant strings (an upstream
    /// extension adding a new variant
    /// must not silently round-trip).
    #[test]
    fn rejects_unknown_variant_string() {
        let result: Result<MessageKind, _> =
            serde_json::from_str("\"nonexistent_kind\"");
        assert!(result.is_err());
    }

    // -- from_role 5-way mapping ------------------------------------

    /// `from_role(System, _)` MUST
    /// return `System` regardless of
    /// the tool-call flag.
    #[test]
    fn from_role_system_ignores_has_tool_calls_flag() {
        assert_eq!(
            MessageKind::from_role(Role::System, false),
            MessageKind::System
        );
        assert_eq!(
            MessageKind::from_role(Role::System, true),
            MessageKind::System
        );
    }

    /// `from_role(User, _)` MUST
    /// return `User` regardless of
    /// the tool-call flag.
    #[test]
    fn from_role_user_ignores_has_tool_calls_flag() {
        assert_eq!(
            MessageKind::from_role(Role::User, false),
            MessageKind::User
        );
        assert_eq!(MessageKind::from_role(Role::User, true), MessageKind::User);
    }

    /// `from_role(Assistant, true)`
    /// MUST return `ToolCall` (the
    /// distinguishing case that
    /// separates plain assistant
    /// text from assistant-with-tools).
    #[test]
    fn from_role_assistant_with_tool_calls_yields_tool_call() {
        assert_eq!(
            MessageKind::from_role(Role::Assistant, true),
            MessageKind::ToolCall
        );
    }

    /// `from_role(Assistant, false)`
    /// MUST return `Assistant` (the
    /// plain text path).
    #[test]
    fn from_role_assistant_without_tool_calls_yields_assistant() {
        assert_eq!(
            MessageKind::from_role(Role::Assistant, false),
            MessageKind::Assistant
        );
    }

    /// `from_role(Tool, _)` MUST
    /// return `ToolResult` regardless
    /// of the tool-call flag.
    #[test]
    fn from_role_tool_ignores_has_tool_calls_flag() {
        assert_eq!(
            MessageKind::from_role(Role::Tool, false),
            MessageKind::ToolResult
        );
        assert_eq!(
            MessageKind::from_role(Role::Tool, true),
            MessageKind::ToolResult
        );
    }

    /// `from_role` MUST cover all 4
    /// `Role` variants across both
    /// `has_tool_calls` values
    /// (8 total combinations).
    #[test]
    fn from_role_covers_all_eight_role_flag_combinations() {
        let all_roles = [
            (Role::System, false),
            (Role::System, true),
            (Role::User, false),
            (Role::User, true),
            (Role::Assistant, false),
            (Role::Assistant, true),
            (Role::Tool, false),
            (Role::Tool, true),
        ];
        for (role, flag) in all_roles {
            let kind = MessageKind::from_role(role, flag);
            // The result MUST be one of the 5 variants
            // (sanity: never panics, always returns).
            let _ = kind;
        }
    }

    // -- Distinctness ------------------------------------------------

    /// All 5 `MessageKind` variants MUST
    /// be pairwise distinct (sanity
    /// check that no two variants
    /// accidentally alias).
    #[test]
    fn all_five_variants_are_pairwise_distinct() {
        let all = [
            MessageKind::System,
            MessageKind::User,
            MessageKind::Assistant,
            MessageKind::ToolCall,
            MessageKind::ToolResult,
        ];
        for i in 0..all.len() {
            for j in 0..all.len() {
                if i != j {
                    assert_ne!(
                        all[i], all[j],
                        "variants {i} ({:?}) and {j} ({:?}) alias",
                        all[i], all[j]
                    );
                }
            }
        }
    }

    // -- Trait surface ----------------------------------------------

    /// `MessageKind` MUST implement
    /// `Hash` (used in HashSet-based
    /// dedup logic in the agent loop).
    #[test]
    fn hash_trait_can_be_used_in_hashset() {
        use std::collections::HashSet;
        let mut s = HashSet::new();
        s.insert(MessageKind::System);
        s.insert(MessageKind::User);
        s.insert(MessageKind::Assistant);
        s.insert(MessageKind::ToolCall);
        s.insert(MessageKind::ToolResult);
        assert_eq!(s.len(), 5);
        assert!(s.contains(&MessageKind::ToolCall));
    }

    /// `MessageKind` MUST implement
    /// `Copy` (used in hot agent-loop
    /// message dispatch).
    #[test]
    fn copy_trait_does_not_move() {
        let k = MessageKind::Assistant;
        let _copy = k;
        let _still_valid = k;
    }
}
