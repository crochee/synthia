//! Message creation and manipulation utilities
//!
//! This module provides helper functions and traits for creating and
//! manipulating sampling messages with common patterns.

use rmcp::model::{
    CallToolResult,
    CreateMessageResult,
    Role,
    SamplingContent,
    SamplingMessage,
    SamplingMessageContent,
    ToolResultContent,
};

/// Create a tool response message
pub fn create_tool_message(
    tool_request_id: String,
    call_tool_result: CallToolResult,
) -> SamplingMessage {
    SamplingMessage {
        role: Role::User,
        content: SamplingContent::Single(SamplingMessageContent::ToolResult(
            ToolResultContent {
                meta: call_tool_result.meta,
                tool_use_id: tool_request_id,
                content: call_tool_result.content,
                structured_content: call_tool_result
                    .structured_content
                    .and_then(|v| v.as_object().cloned()),
                is_error: call_tool_result.is_error,
            },
        )),
        meta: None,
    }
}

/// Extract all ToolUseContent from a SamplingMessage
pub fn extract_tool_uses(
    msg: &SamplingMessage,
) -> Vec<rmcp::model::ToolUseContent> {
    match &msg.content {
        SamplingContent::Single(SamplingMessageContent::ToolUse(tool_use)) => {
            vec![tool_use.clone()]
        }
        SamplingContent::Multiple(contents) => contents
            .iter()
            .filter_map(|c| {
                if let SamplingMessageContent::ToolUse(tool_use) = c {
                    Some(tool_use.clone())
                } else {
                    None
                }
            })
            .collect(),
        _ => vec![],
    }
}

/// Extract text content from a SamplingMessage
pub fn extract_text_content(msg: &SamplingMessage) -> String {
    match &msg.content {
        SamplingContent::Single(content) => content_to_string(content),
        SamplingContent::Multiple(contents) => contents
            .iter()
            .map(content_to_string)
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// Extract text parts from a SamplingMessage, returning None if no text content
pub fn extract_text_parts(msg: &SamplingMessage) -> Option<String> {
    let text_parts: Vec<String> = msg
        .content
        .iter()
        .filter_map(|c| {
            if let SamplingMessageContent::Text(text) = c {
                Some(text.text.clone())
            } else {
                None
            }
        })
        .collect();

    if text_parts.is_empty() {
        None
    } else {
        Some(text_parts.join("\n"))
    }
}

/// Find the most recent message with the specified role that contains only text
pub fn find_recent_text_message(
    conversation: &[SamplingMessage],
    role: Role,
) -> Option<&SamplingMessage> {
    conversation
        .iter()
        .rev()
        .find(|msg| msg.role == role && has_text_only(msg))
}

/// Check if a message contains only text content (no tool use/result)
pub(crate) fn has_text_only(msg: &SamplingMessage) -> bool {
    let has_text = msg
        .content
        .iter()
        .any(|c| matches!(c, SamplingMessageContent::Text(_)));
    let has_tool_content = msg.content.iter().any(|c| {
        matches!(
            c,
            SamplingMessageContent::ToolResult(_)
                | SamplingMessageContent::ToolUse(_)
        )
    });
    has_text && !has_tool_content
}

/// Extract text from a CreateMessageResult, excluding reasoning content
pub fn extract_text_from_result(result: &CreateMessageResult) -> String {
    result
        .message
        .content
        .iter()
        .filter_map(|content| {
            let SamplingMessageContent::Text(text) = content else {
                return None;
            };
            let is_reasoning = text
                .meta
                .as_ref()
                .and_then(|m| m.0.get("type"))
                .and_then(|v| v.as_str())
                == Some("reasoning");
            if is_reasoning {
                None
            } else {
                Some(text.text.clone())
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract the response text from a list of messages (most recent assistant message)
pub fn extract_response_text(messages: &[SamplingMessage]) -> String {
    find_recent_text_message(messages, Role::Assistant)
        .and_then(extract_text_parts)
        .unwrap_or_else(|| "No response from subagent".to_string())
}

/// Convert SamplingMessageContent to a string representation
pub fn content_to_string(content: &SamplingMessageContent) -> String {
    match content {
        SamplingMessageContent::Text(text) => text.text.clone(),
        SamplingMessageContent::ToolResult(tr) => {
            format!("Tool result for {}", tr.tool_use_id)
        }
        SamplingMessageContent::ToolUse(tu) => format!("Tool use: {}", tu.name),
        SamplingMessageContent::Image(_) => "[Image]".into(),
        SamplingMessageContent::Audio(_) => "[Audio]".into(),
    }
}

/// Convert SamplingContent to string
pub fn sampling_content_to_string(
    content: &SamplingContent<SamplingMessageContent>,
) -> String {
    match content {
        SamplingContent::Single(c) => content_to_string(c),
        SamplingContent::Multiple(cs) => cs
            .iter()
            .map(content_to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// Extract text content from a SamplingMessage (handles Single content only).
/// Returns empty string for non-text content types.
pub fn extract_text(msg: &SamplingMessage) -> String {
    match &msg.content {
        SamplingContent::Single(SamplingMessageContent::Text(t)) => {
            t.text.clone()
        }
        _ => String::new(),
    }
}

/// Convert SamplingMessage to string
pub fn message_to_string(msg: &SamplingMessage) -> String {
    let role = match msg.role {
        Role::User => "User",
        Role::Assistant => "Assistant",
    };
    format!("{role}: {}", sampling_content_to_string(&msg.content))
}

#[cfg(test)]
mod tests {
    use rmcp::model::RawTextContent;

    use super::*;

    #[test]
    fn test_extract_text_parts_single() {
        let msg = SamplingMessage::user_text("Hello world");
        assert_eq!(extract_text_parts(&msg), Some("Hello world".to_string()));
    }

    #[test]
    fn test_extract_text_parts_empty() {
        let msg = SamplingMessage {
            role: Role::User,
            content: SamplingContent::Multiple(vec![]),
            meta: None,
        };
        assert_eq!(extract_text_parts(&msg), None);
    }

    #[test]
    fn test_has_text_only() {
        let msg = SamplingMessage::user_text("Hello");
        assert!(has_text_only(&msg));
    }

    #[test]
    fn test_extract_tool_uses_single() {
        let tool_use = rmcp::model::ToolUseContent::new(
            "tool_123",
            "test_tool",
            serde_json::json!({})
                .as_object()
                .cloned()
                .unwrap_or_default(),
        );
        let msg = SamplingMessage {
            role: Role::Assistant,
            content: SamplingContent::Single(SamplingMessageContent::ToolUse(
                tool_use,
            )),
            meta: None,
        };
        let result = extract_tool_uses(&msg);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "tool_123");
    }

    #[test]
    fn test_extract_tool_uses_multiple() {
        let tool1 = rmcp::model::ToolUseContent::new(
            "tool_1",
            "tool_a",
            serde_json::json!({})
                .as_object()
                .cloned()
                .unwrap_or_default(),
        );
        let tool2 = rmcp::model::ToolUseContent::new(
            "tool_2",
            "tool_b",
            serde_json::json!({})
                .as_object()
                .cloned()
                .unwrap_or_default(),
        );
        let msg = SamplingMessage {
            role: Role::Assistant,
            content: SamplingContent::Multiple(vec![
                SamplingMessageContent::ToolUse(tool1),
                SamplingMessageContent::Text(RawTextContent {
                    text: "hello".into(),
                    meta: None,
                }),
                SamplingMessageContent::ToolUse(tool2),
            ]),
            meta: None,
        };
        let result = extract_tool_uses(&msg);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, "tool_1");
        assert_eq!(result[1].id, "tool_2");
    }

    #[test]
    fn test_extract_tool_uses_none() {
        let msg = SamplingMessage::user_text("Just text");
        let result = extract_tool_uses(&msg);
        assert!(result.is_empty());
    }

    #[test]
    fn test_extract_text_content_single() {
        let msg = SamplingMessage::user_text("Hello world");
        assert_eq!(extract_text_content(&msg), "Hello world");
    }

    #[test]
    fn test_extract_text_content_multiple() {
        let msg = SamplingMessage {
            role: Role::User,
            content: SamplingContent::Multiple(vec![
                SamplingMessageContent::Text(RawTextContent {
                    text: "Hello".into(),
                    meta: None,
                }),
                SamplingMessageContent::Text(RawTextContent {
                    text: "World".into(),
                    meta: None,
                }),
            ]),
            meta: None,
        };
        assert_eq!(extract_text_content(&msg), "Hello World");
    }

    #[test]
    fn test_find_recent_text_message_user() {
        let messages = vec![
            SamplingMessage::user_text("First"),
            SamplingMessage::assistant_text("Second"),
            SamplingMessage::user_text("Third"),
        ];
        let result = find_recent_text_message(&messages, Role::User);
        assert!(
            extract_text_parts(result.unwrap()).as_deref() == Some("Third")
        );
    }

    #[test]
    fn test_find_recent_text_message_assistant() {
        let messages = vec![
            SamplingMessage::user_text("First"),
            SamplingMessage::assistant_text("Second"),
            SamplingMessage::user_text("Third"),
        ];
        let result = find_recent_text_message(&messages, Role::Assistant);
        assert!(
            extract_text_parts(result.unwrap()).as_deref() == Some("Second")
        );
    }

    #[test]
    fn test_find_recent_text_message_none() {
        let messages = vec![SamplingMessage::user_text("First")];
        let result = find_recent_text_message(&messages, Role::Assistant);
        assert!(result.is_none());
    }

    #[test]
    fn test_content_to_string_text() {
        let content = SamplingMessageContent::Text(RawTextContent {
            text: "Hello".into(),
            meta: None,
        });
        assert_eq!(content_to_string(&content), "Hello");
    }

    #[test]
    fn test_content_to_string_tool_result() {
        let content = SamplingMessageContent::ToolResult(
            rmcp::model::ToolResultContent {
                tool_use_id: "tool_123".into(),
                content: vec![],
                meta: None,
                structured_content: None,
                is_error: Some(false),
            },
        );
        assert_eq!(content_to_string(&content), "Tool result for tool_123");
    }

    #[test]
    fn test_content_to_string_tool_use() {
        let content =
            SamplingMessageContent::ToolUse(rmcp::model::ToolUseContent::new(
                "tool_456",
                "my_tool",
                serde_json::json!({})
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
            ));
        assert_eq!(content_to_string(&content), "Tool use: my_tool");
    }

    #[test]
    fn test_sampling_content_to_string_single() {
        let content = SamplingContent::Single(SamplingMessageContent::Text(
            RawTextContent {
                text: "Hello".into(),
                meta: None,
            },
        ));
        assert_eq!(sampling_content_to_string(&content), "Hello");
    }

    #[test]
    fn test_sampling_content_to_string_multiple() {
        let content = SamplingContent::Multiple(vec![
            SamplingMessageContent::Text(RawTextContent {
                text: "Hello".into(),
                meta: None,
            }),
            SamplingMessageContent::Text(RawTextContent {
                text: "World".into(),
                meta: None,
            }),
        ]);
        assert_eq!(sampling_content_to_string(&content), "Hello\nWorld");
    }

    #[test]
    fn test_extract_text() {
        let msg = SamplingMessage::user_text("Direct text");
        assert_eq!(extract_text(&msg), "Direct text");
    }

    #[test]
    fn test_extract_text_non_text() {
        let tool_use = rmcp::model::ToolUseContent::new(
            "tool_123",
            "test",
            serde_json::json!({})
                .as_object()
                .cloned()
                .unwrap_or_default(),
        );
        let msg = SamplingMessage {
            role: Role::Assistant,
            content: SamplingContent::Single(SamplingMessageContent::ToolUse(
                tool_use,
            )),
            meta: None,
        };
        assert_eq!(extract_text(&msg), "");
    }

    #[test]
    fn test_message_to_string_user() {
        let msg = SamplingMessage::user_text("Hello");
        assert_eq!(message_to_string(&msg), "User: Hello");
    }

    #[test]
    fn test_message_to_string_assistant() {
        let msg = SamplingMessage::assistant_text("Hello");
        assert_eq!(message_to_string(&msg), "Assistant: Hello");
    }

    #[test]
    fn test_extract_response_text_with_assistant() {
        let messages = vec![
            SamplingMessage::user_text("First"),
            SamplingMessage::assistant_text("Response text"),
        ];
        assert_eq!(extract_response_text(&messages), "Response text");
    }

    #[test]
    fn test_extract_response_text_empty() {
        let messages: Vec<SamplingMessage> = vec![];
        assert_eq!(
            extract_response_text(&messages),
            "No response from subagent"
        );
    }
}
