//! History normalization module
//!
//! This module provides functions to normalize conversation history by ensuring
//! tool call/output pairs are properly matched and removing orphaned items.

use rmcp::model::{
    RawTextContent,
    SamplingContent,
    SamplingMessage,
    SamplingMessageContent,
};

/// Ensure that every tool use has a corresponding tool result
pub fn ensure_call_outputs_present(messages: &mut Vec<SamplingMessage>) {
    let mut i = 0;
    while i < messages.len() {
        // Check if current message is a tool use and get its id
        let tool_use_id = if let SamplingContent::Single(
            SamplingMessageContent::ToolUse(tool_use),
        ) = &messages[i].content
        {
            Some(tool_use.id.clone())
        } else {
            None
        };

        if let Some(tool_use_id) = tool_use_id {
            // Check if next message is a tool result
            let has_corresponding_output = (i + 1 < messages.len()) && {
                matches!(
                    &messages[i + 1].content,
                    SamplingContent::Single(
                        SamplingMessageContent::ToolResult(_)
                    )
                )
            };

            if !has_corresponding_output {
                // Insert a placeholder tool result
                // IMPORTANT: Tool results must have Role::User, not Assistant
                let placeholder_result = SamplingMessage {
                    role: rmcp::model::Role::User,
                    content: SamplingContent::Single(
                        SamplingMessageContent::ToolResult(
                            rmcp::model::ToolResultContent::new(
                                &tool_use_id,
                                vec![rmcp::model::Content::text(
                                    "[Tool execution placeholder - no response received]",
                                )],
                            ),
                        ),
                    ),
                    meta: None,
                };
                messages.insert(i + 1, placeholder_result);
                i += 1; // Skip the newly inserted placeholder
            }
        }
        i += 1;
    }
}

/// Remove orphaned tool results (results without corresponding tool use)
pub fn remove_orphan_outputs(messages: &mut Vec<SamplingMessage>) {
    let mut to_remove = Vec::new();

    for (i, message) in messages.iter().enumerate() {
        // Check if current message is a tool result
        let is_tool_result = matches!(
            &message.content,
            SamplingContent::Single(SamplingMessageContent::ToolResult(_))
        );

        if is_tool_result {
            // Check if previous message is a tool use
            let has_corresponding_input = i > 0 && {
                matches!(
                    &messages[i - 1].content,
                    SamplingContent::Single(SamplingMessageContent::ToolUse(_))
                )
            };

            if !has_corresponding_input {
                to_remove.push(i);
            }
        }
    }

    // Remove in reverse order to avoid index shifting
    for &i in to_remove.iter().rev() {
        messages.remove(i);
    }
}

/// Strip images from messages when model doesn't support them
pub fn strip_images_when_unsupported(
    messages: &mut Vec<SamplingMessage>,
    image_supported: bool,
) {
    if image_supported {
        return;
    }

    for message in messages {
        match &mut message.content {
            SamplingContent::Single(content) => {
                if matches!(content, SamplingMessageContent::Image(_)) {
                    // Replace image with placeholder text
                    *content = SamplingMessageContent::Text(RawTextContent {
                        text: "[Image content - not supported by model]"
                            .to_string(),
                        meta: None,
                    });
                }
            }
            SamplingContent::Multiple(contents) => {
                contents.retain(|content| {
                    !matches!(content, SamplingMessageContent::Image(_))
                });
                // If all contents were images, add a placeholder
                if contents.is_empty() {
                    contents.push(SamplingMessageContent::Text(
                        RawTextContent {
                            text: "[Image content - not supported by model]"
                                .to_string(),
                            meta: None,
                        },
                    ));
                }
            }
        }
    }
}

/// Normalize the entire history
pub fn normalize_history(
    messages: &mut Vec<SamplingMessage>,
    image_supported: bool,
) {
    // Ensure all tool uses have corresponding outputs
    ensure_call_outputs_present(messages);

    // Remove orphaned tool results
    remove_orphan_outputs(messages);

    // Strip images if not supported
    strip_images_when_unsupported(messages, image_supported);
}

#[cfg(test)]
mod tests {
    use rmcp::model::{
        Content,
        RawTextContent,
        Role,
        SamplingContent,
        SamplingMessage,
        SamplingMessageContent,
        ToolResultContent,
        ToolUseContent,
    };

    use super::*;

    fn _create_text_message(role: Role, text: &str) -> SamplingMessage {
        SamplingMessage {
            role,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: text.to_string(),
                    meta: None,
                },
            )),
            meta: None,
        }
    }

    fn create_tool_use_message(id: &str, name: &str) -> SamplingMessage {
        SamplingMessage {
            role: Role::Assistant,
            content: SamplingContent::Single(SamplingMessageContent::ToolUse(
                ToolUseContent::new(
                    id,
                    name,
                    serde_json::json!({})
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                ),
            )),
            meta: None,
        }
    }

    fn create_tool_result_message(
        id: &str,
        items: Vec<&str>,
    ) -> SamplingMessage {
        let content: Vec<Content> = items
            .iter()
            .map(|item| Content::text(item.to_string()))
            .collect();

        SamplingMessage {
            role: Role::User,
            content: SamplingContent::Single(
                SamplingMessageContent::ToolResult(ToolResultContent::new(
                    id, content,
                )),
            ),
            meta: None,
        }
    }

    fn create_image_message(role: Role) -> SamplingMessage {
        SamplingMessage {
            role,
            content: SamplingContent::Single(SamplingMessageContent::Image(
                rmcp::model::RawImageContent {
                    data: "base64data".to_string(),
                    mime_type: "image/png".to_string(),
                    meta: None,
                },
            )),
            meta: None,
        }
    }

    // =============================================================================
    // ensure_call_outputs_present tests
    // =============================================================================

    #[test]
    fn test_ensure_call_outputs_present_no_action_needed() {
        let mut messages = vec![
            create_tool_use_message("tool-1", "test_tool"),
            create_tool_result_message("tool-1", vec!["Result"]),
        ];

        ensure_call_outputs_present(&mut messages);

        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_ensure_call_outputs_present_inserts_placeholder() {
        let mut messages = vec![create_tool_use_message("tool-1", "test_tool")];

        ensure_call_outputs_present(&mut messages);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].role, Role::User);

        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &messages[1].content
        {
            assert!(
                result.content[0]
                    .as_text()
                    .unwrap()
                    .text
                    .contains("placeholder")
            );
        } else {
            panic!("Expected ToolResult content");
        }
    }

    #[test]
    fn test_ensure_call_outputs_present_multiple_tools() {
        let mut messages = vec![
            create_tool_use_message("tool-1", "tool_a"),
            create_tool_use_message("tool-2", "tool_b"),
        ];

        ensure_call_outputs_present(&mut messages);

        // Should insert two placeholders
        assert_eq!(messages.len(), 4);
        // First placeholder after tool-1
        assert_eq!(messages[1].role, Role::User);
        // Second placeholder after tool-2
        assert_eq!(messages[3].role, Role::User);
    }

    // =============================================================================
    // remove_orphan_outputs tests
    // =============================================================================

    #[test]
    fn test_remove_orphan_outputs_none_to_remove() {
        let mut messages = vec![
            create_tool_use_message("tool-1", "test_tool"),
            create_tool_result_message("tool-1", vec!["Result"]),
        ];

        remove_orphan_outputs(&mut messages);

        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_remove_orphan_outputs_removes_orphans() {
        let mut messages = vec![
            create_tool_result_message("tool-1", vec!["Orphan result"]),
            create_tool_use_message("tool-1", "test_tool"),
            create_tool_result_message("tool-1", vec!["Result"]),
        ];

        remove_orphan_outputs(&mut messages);

        // Orphan at index 0 should be removed
        assert_eq!(messages.len(), 2);
        assert!(matches!(
            &messages[0].content,
            SamplingContent::Single(SamplingMessageContent::ToolUse(_))
        ));
    }

    #[test]
    fn test_remove_orphan_outputs_multiple_orphans() {
        let mut messages = vec![
            create_tool_result_message("orphan-1", vec!["Orphan"]),
            create_tool_use_message("tool-1", "test_tool"),
            create_tool_result_message("tool-1", vec!["Result"]),
            create_tool_result_message("orphan-2", vec!["Orphan"]),
        ];

        remove_orphan_outputs(&mut messages);

        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_remove_orphan_outputs_empty() {
        let mut messages: Vec<SamplingMessage> = Vec::new();
        remove_orphan_outputs(&mut messages);
        assert_eq!(messages.len(), 0);
    }

    // =============================================================================
    // strip_images_when_unsupported tests
    // =============================================================================

    #[test]
    fn test_strip_images_when_supported_no_change() {
        let mut messages = vec![create_image_message(Role::User)];

        strip_images_when_unsupported(&mut messages, true);

        // Should not modify when supported
        assert!(matches!(
            &messages[0].content,
            SamplingContent::Single(SamplingMessageContent::Image(_))
        ));
    }

    #[test]
    fn test_strip_images_when_unsupported_replaces_with_placeholder() {
        let mut messages = vec![create_image_message(Role::User)];

        strip_images_when_unsupported(&mut messages, false);

        assert!(matches!(
            &messages[0].content,
            SamplingContent::Single(SamplingMessageContent::Text(_))
        ));
        if let SamplingContent::Single(SamplingMessageContent::Text(t)) =
            &messages[0].content
        {
            assert!(t.text.contains("not supported"));
        }
    }

    #[test]
    fn test_strip_images_multiple_in_message() {
        let mut messages = vec![SamplingMessage {
            role: Role::User,
            content: SamplingContent::Multiple(vec![
                SamplingMessageContent::Image(rmcp::model::RawImageContent {
                    data: "base64data".to_string(),
                    mime_type: "image/png".to_string(),
                    meta: None,
                }),
                SamplingMessageContent::Image(rmcp::model::RawImageContent {
                    data: "base64data2".to_string(),
                    mime_type: "image/jpeg".to_string(),
                    meta: None,
                }),
            ]),
            meta: None,
        }];

        strip_images_when_unsupported(&mut messages, false);

        // Multiple images should be retained but replaced with placeholder text
        if let SamplingContent::Multiple(contents) = &messages[0].content {
            assert!(!contents.is_empty());
            // At least one should be text (placeholder)
            assert!(
                contents
                    .iter()
                    .any(|c| matches!(c, SamplingMessageContent::Text(_)))
            );
        }
    }

    // =============================================================================
    // normalize_history tests
    // =============================================================================

    #[test]
    fn test_normalize_history_empty() {
        let mut messages: Vec<SamplingMessage> = Vec::new();
        normalize_history(&mut messages, true);
        assert_eq!(messages.len(), 0);
    }

    #[test]
    fn test_normalize_history_combined_issues() {
        // Tool use without output, orphan result, and unsupported image
        let mut messages = vec![
            create_tool_result_message("orphan", vec!["Orphan"]),
            create_tool_use_message("tool-1", "test_tool"),
            create_image_message(Role::User),
        ];

        normalize_history(&mut messages, false);

        // Orphan removed (1), placeholder inserted (1), image replaced (1)
        // Original: 3 messages, after: orphan removed + placeholder + image replacement
        // Messages: [orphan result removed, tool_use, placeholder inserted, text placeholder for image]
        assert_eq!(messages.len(), 3);
    }
}
