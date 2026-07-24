//! Message creation and manipulation utilities
//!
//! This module provides helper functions and traits for creating and
//! manipulating messages with common patterns.

use synthia_provider::{Content, ContentPart, Message, Role};

/// Extract text content from a Message
pub fn extract_text_content(msg: &Message) -> String {
    msg.content.extract_text().unwrap_or_default()
}

/// Find the most recent message with the specified role that contains only text
pub fn find_recent_text_message(
    conversation: &[Message],
    role: Role,
) -> Option<&Message> {
    conversation
        .iter()
        .rev()
        .find(|msg| msg.role == role && has_text_only(msg))
}

/// Check if a message contains only text content (no tool use/result)
pub(crate) fn has_text_only(msg: &Message) -> bool {
    let has_text = msg.content.has_text();
    let has_tool_content = msg.content.has_tool_use();
    has_text && !has_tool_content
}

/// Extract the response text from a list of messages (most recent assistant message)
pub fn extract_response_text(messages: &[Message]) -> String {
    find_recent_text_message(messages, Role::Assistant)
        .and_then(|msg| msg.content.extract_text())
        .unwrap_or_else(|| "No response from subagent".to_string())
}

/// Extract text parts from a Message, returning None if no text content
pub fn extract_text_parts(msg: &Message) -> Option<String> {
    msg.content.extract_text()
}

/// Convert ContentPart to a string representation
pub(crate) fn content_part_to_string(content: &ContentPart) -> String {
    match content {
        ContentPart::Text(t) => t.text.clone(),
        ContentPart::ToolResult(tr) => {
            let texts: Vec<String> = tr
                .content
                .iter()
                .filter_map(|c| c.text().map(std::string::ToString::to_string))
                .collect();
            texts.join(" ")
        }
        ContentPart::ToolUse(tu) => format!("Tool use: {}", tu.name),
        ContentPart::Image(_) => "[Image]".into(),
        ContentPart::Audio(_) => "[Audio]".into(),
        ContentPart::Reasoning(t) => t.text.clone(),
        ContentPart::Resource(_) => "[Resource]".into(),
    }
}

/// Convert Content to string
pub fn content_to_string(content: &Content) -> String {
    let parts: Vec<String> =
        content.into_iter().map(content_part_to_string).collect();
    parts.join("\n")
}

/// Convert Message to string
pub fn message_to_string(msg: &Message) -> String {
    let role = match msg.role {
        Role::User => "User",
        Role::Assistant => "Assistant",
        Role::System => "System",
        Role::Tool => "Tool",
    };
    format!("{role}: {}", content_to_string(&msg.content))
}

/// Extract tool uses from a message
pub fn extract_tool_uses(msg: &Message) -> Vec<synthia_provider::ToolUse> {
    msg.content.extract_tool_uses()
}

/// Create a tool message
pub fn create_tool_message(
    tool_use_id: impl Into<String>,
    content: impl Into<String>,
) -> Message {
    Message::tool(Content::text(content), tool_use_id)
}

/// Extract text from a Message (handles Single content only).
/// Returns empty string for non-text content types.
pub fn extract_text(msg: &Message) -> String {
    msg.content.extract_text().unwrap_or_default()
}

/// Extract text from a Message, excluding reasoning content
pub fn extract_text_from_result(result: &Message) -> String {
    let texts: Vec<String> = (&result.content)
        .into_iter()
        .filter_map(|content| {
            if let ContentPart::Text(t) = content {
                let is_reasoning = t.text.contains("[Reasoning:");
                if is_reasoning {
                    None
                } else {
                    Some(t.text.clone())
                }
            } else {
                None
            }
        })
        .collect();
    texts.join("\n")
}

/// Alias for content_to_string for backwards compatibility
pub fn sampling_content_to_string(content: &Content) -> String {
    content_to_string(content)
}

#[cfg(test)]
mod tests {
    use synthia_provider::{Content, ContentPart, Message, Role, TextContent};

    use super::*;

    #[test]
    fn test_extract_text_parts_single() {
        let msg = Message::user("Hello world");
        assert_eq!(extract_text_parts(&msg), Some("Hello world".to_string()));
    }

    #[test]
    fn test_extract_text_parts_empty() {
        let msg = Message {
            role: Role::User,
            content: Content::Multi(vec![]),
            tool_call_id: None,
            name: None,
            ..Default::default()
        };
        assert_eq!(extract_text_parts(&msg), None);
    }

    #[test]
    fn test_has_text_only() {
        let msg = Message::user("Hello");
        assert!(has_text_only(&msg));
    }

    #[test]
    fn test_extract_tool_uses_single() {
        let tool_use = synthia_provider::ToolUse {
            id: "tool_123".into(),
            name: "test_tool".into(),
            input: serde_json::json!({}),
        };
        let msg = Message {
            role: Role::Assistant,
            content: Content::Single(ContentPart::ToolUse(tool_use)),
            tool_call_id: None,
            name: None,
            ..Default::default()
        };
        let result = extract_tool_uses(&msg);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "tool_123");
    }

    #[test]
    fn test_extract_tool_uses_multiple() {
        let tool1 = synthia_provider::ToolUse {
            id: "tool_1".into(),
            name: "tool_a".into(),
            input: serde_json::json!({}),
        };
        let tool2 = synthia_provider::ToolUse {
            id: "tool_2".into(),
            name: "tool_b".into(),
            input: serde_json::json!({}),
        };
        let msg = Message {
            role: Role::Assistant,
            content: Content::Multi(vec![
                ContentPart::ToolUse(tool1),
                ContentPart::Text(TextContent {
                    text: "hello".into(),
                    cache_control: None,
                }),
                ContentPart::ToolUse(tool2),
            ]),
            tool_call_id: None,
            name: None,
            ..Default::default()
        };
        let result = extract_tool_uses(&msg);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, "tool_1");
        assert_eq!(result[1].id, "tool_2");
    }

    #[test]
    fn test_extract_tool_uses_none() {
        let msg = Message::user("Just text");
        let result = extract_tool_uses(&msg);
        assert!(result.is_empty());
    }

    #[test]
    fn test_extract_text_content_single() {
        let msg = Message::user("Hello world");
        assert_eq!(extract_text_content(&msg), "Hello world");
    }

    #[test]
    fn test_extract_text_content_multiple() {
        let msg = Message {
            role: Role::User,
            content: Content::Multi(vec![
                ContentPart::Text(TextContent {
                    text: "Hello".into(),
                    cache_control: None,
                }),
                ContentPart::Text(TextContent {
                    text: "World".into(),
                    cache_control: None,
                }),
            ]),
            tool_call_id: None,
            name: None,
            ..Default::default()
        };
        assert_eq!(extract_text_content(&msg), "HelloWorld");
    }

    #[test]
    fn test_find_recent_text_message_user() {
        let messages = vec![
            Message::user("First"),
            Message::assistant("Second"),
            Message::user("Third"),
        ];
        let result = find_recent_text_message(&messages, Role::User);
        assert!(
            extract_text_parts(result.unwrap()).as_deref() == Some("Third")
        );
    }

    #[test]
    fn test_find_recent_text_message_assistant() {
        let messages = vec![
            Message::user("First"),
            Message::assistant("Second"),
            Message::user("Third"),
        ];
        let result = find_recent_text_message(&messages, Role::Assistant);
        assert!(
            extract_text_parts(result.unwrap()).as_deref() == Some("Second")
        );
    }

    #[test]
    fn test_find_recent_text_message_none() {
        let messages = vec![Message::user("First")];
        let result = find_recent_text_message(&messages, Role::Assistant);
        assert!(result.is_none());
    }

    #[test]
    fn test_content_to_string_text() {
        let content = ContentPart::Text(TextContent {
            text: "Hello".into(),
            cache_control: None,
        });
        assert_eq!(content_part_to_string(&content), "Hello");
    }

    #[test]
    fn test_content_to_string_tool_result() {
        let content = ContentPart::ToolResult(synthia_provider::ToolResult {
            tool_use_id: "tool_123".into(),
            content: vec![],
            structured_content: None,
            is_error: Some(false),
        });
        assert_eq!(content_part_to_string(&content), "");
    }

    #[test]
    fn test_content_to_string_tool_use() {
        let content = ContentPart::ToolUse(synthia_provider::ToolUse {
            id: "tool_456".into(),
            name: "my_tool".into(),
            input: serde_json::json!({}),
        });
        assert_eq!(content_part_to_string(&content), "Tool use: my_tool");
    }

    #[test]
    fn test_content_to_string_single() {
        let content = Content::Single(ContentPart::Text(TextContent {
            text: "Hello".into(),
            cache_control: None,
        }));
        assert_eq!(content_to_string(&content), "Hello");
    }

    #[test]
    fn test_content_to_string_multiple() {
        let content = Content::Multi(vec![
            ContentPart::Text(TextContent {
                text: "Hello".into(),
                cache_control: None,
            }),
            ContentPart::Text(TextContent {
                text: "World".into(),
                cache_control: None,
            }),
        ]);
        assert_eq!(content_to_string(&content), "Hello\nWorld");
    }

    #[test]
    fn test_message_to_string_user() {
        let msg = Message::user("Hello");
        assert_eq!(message_to_string(&msg), "User: Hello");
    }

    #[test]
    fn test_message_to_string_assistant() {
        let msg = Message::assistant("Hello");
        assert_eq!(message_to_string(&msg), "Assistant: Hello");
    }

    #[test]
    fn test_extract_response_text_with_assistant() {
        let messages =
            vec![Message::user("First"), Message::assistant("Response text")];
        assert_eq!(extract_response_text(&messages), "Response text");
    }

    #[test]
    fn test_extract_response_text_empty() {
        let messages: Vec<Message> = vec![];
        assert_eq!(
            extract_response_text(&messages),
            "No response from subagent"
        );
    }
}
