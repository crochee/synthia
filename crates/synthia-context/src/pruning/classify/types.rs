//! Message classification types and helpers for pruning.

use synthia_provider::{ContentPart, Message, Role};

/// Message classification for intelligent context management
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageClassification {
    /// User text message (should always be preserved)
    UserText,
    /// Assistant message with text content
    AssistantText,
    /// Tool use message (part of a tool pair)
    ToolUse,
    /// Tool result message (part of a tool pair)
    ToolResult,
    /// Other message types
    Other,
}

/// Check if a message is a user text message (not a tool result)
pub fn is_user_text_message(msg: &Message) -> bool {
    if msg.role != Role::User {
        return false;
    }

    // Check if this is a tool result message
    let is_tool_result = (&msg.content)
        .into_iter()
        .any(|c| matches!(c, ContentPart::ToolResult(_)));

    !is_tool_result
}

pub fn is_tool_use(msg: &Message) -> bool {
    (&msg.content)
        .into_iter()
        .any(|c| matches!(c, ContentPart::ToolUse(_)))
}

pub fn is_tool_result(msg: &Message) -> bool {
    (&msg.content)
        .into_iter()
        .any(|c| matches!(c, ContentPart::ToolResult(_)))
}

pub fn get_tool_use_id(msg: &Message) -> Option<String> {
    (&msg.content).into_iter().find_map(|c| match c {
        ContentPart::ToolUse(tu) => Some(tu.id.clone()),
        _ => None,
    })
}

pub fn get_tool_result_id(msg: &Message) -> Option<String> {
    (&msg.content).into_iter().find_map(|c| match c {
        ContentPart::ToolResult(tr) => Some(tr.tool_use_id.clone()),
        _ => None,
    })
}

pub fn classify_messages(messages: &[Message]) -> Vec<MessageClassification> {
    messages.iter().map(classify_message).collect()
}

pub(crate) fn classify_message(msg: &Message) -> MessageClassification {
    if is_tool_use(msg) {
        MessageClassification::ToolUse
    } else if is_tool_result(msg) {
        MessageClassification::ToolResult
    } else if msg.role == Role::User && is_user_text_message(msg) {
        MessageClassification::UserText
    } else if msg.role == Role::Assistant {
        MessageClassification::AssistantText
    } else {
        MessageClassification::Other
    }
}
