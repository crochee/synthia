//! Tool pruning strategies with importance-based differentiation

use rmcp::model::{
    Role,
    SamplingContent,
    SamplingMessage,
    SamplingMessageContent,
};

/// Message classification for intelligent context management
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MessageClassification {
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
pub(crate) fn is_user_text_message(msg: &SamplingMessage) -> bool {
    if msg.role != Role::User {
        return false;
    }

    // Check if this is a tool result message
    let is_tool_result = msg
        .content
        .iter()
        .any(|c| matches!(c, SamplingMessageContent::ToolResult(_)));

    // User text message = User role + not a tool result
    !is_tool_result
}

/// Check if a message is a tool use
pub(crate) fn is_tool_use(msg: &SamplingMessage) -> bool {
    msg.content
        .iter()
        .any(|c| matches!(c, SamplingMessageContent::ToolUse(_)))
}

/// Check if a message is a tool result
pub(crate) fn is_tool_result(msg: &SamplingMessage) -> bool {
    msg.content
        .iter()
        .any(|c| matches!(c, SamplingMessageContent::ToolResult(_)))
}

/// Get tool use ID from a message if it exists
pub(crate) fn get_tool_use_id(msg: &SamplingMessage) -> Option<String> {
    msg.content.iter().find_map(|c| {
        if let SamplingMessageContent::ToolUse(tool_use) = c {
            Some(tool_use.id.clone())
        } else {
            None
        }
    })
}

/// Get tool result ID from a message if it exists
pub(crate) fn get_tool_result_id(msg: &SamplingMessage) -> Option<String> {
    msg.content.iter().find_map(|c| {
        if let SamplingMessageContent::ToolResult(result) = c {
            Some(result.tool_use_id.clone())
        } else {
            None
        }
    })
}

/// Classify messages into categories and identify tool pairs
pub(crate) fn classify_messages(
    messages: &[SamplingMessage],
) -> Vec<(usize, MessageClassification)> {
    messages
        .iter()
        .enumerate()
        .map(|(idx, msg)| {
            let classification =
                match msg.content.iter().find_map(|c| match c {
                    SamplingMessageContent::ToolUse(_) => {
                        Some(MessageClassification::ToolUse)
                    }
                    SamplingMessageContent::ToolResult(_) => {
                        Some(MessageClassification::ToolResult)
                    }
                    _ => None,
                }) {
                    Some(classification) => classification,
                    None if is_user_text_message(msg) => {
                        MessageClassification::UserText
                    }
                    None if msg.role == Role::Assistant => {
                        MessageClassification::AssistantText
                    }
                    None => MessageClassification::Other,
                };
            (idx, classification)
        })
        .collect()
}

/// Find the index of the ToolUse message corresponding to a ToolResult
pub(crate) fn find_tool_use_for_result(
    messages: &[SamplingMessage],
    result_idx: usize,
) -> Option<usize> {
    let result_id = get_tool_result_id(messages.get(result_idx)?)?;

    messages[..result_idx]
        .iter()
        .rposition(|msg| get_tool_use_id(msg).as_ref() == Some(&result_id))
}

/// Find the index of the ToolResult message corresponding to a ToolUse
pub(crate) fn find_result_for_tool_use(
    messages: &[SamplingMessage],
    use_idx: usize,
) -> Option<usize> {
    let use_id = get_tool_use_id(messages.get(use_idx)?)?;

    messages[use_idx + 1..]
        .iter()
        .position(|msg| get_tool_result_id(msg).as_ref() == Some(&use_id))
        .map(|i| use_idx + 1 + i)
}

/// Extract tool use IDs from message content
fn extract_tool_use_ids(msg: &SamplingMessage) -> Vec<String> {
    msg.content
        .iter()
        .filter_map(|c| {
            if let SamplingMessageContent::ToolUse(tool_use) = c {
                Some(tool_use.id.clone())
            } else {
                None
            }
        })
        .collect()
}

/// Extract tool result IDs from message content
fn extract_tool_result_ids(msg: &SamplingMessage) -> Vec<String> {
    msg.content
        .iter()
        .filter_map(|c| {
            if let SamplingMessageContent::ToolResult(result) = c {
                Some(result.tool_use_id.clone())
            } else {
                None
            }
        })
        .collect()
}

/// Prune tool result by level, keeping first and last items with a truncated hint
fn prune_tool_result_by_level(
    result: &rmcp::model::ToolResultContent,
    truncated_hint: &str,
) -> rmcp::model::ToolResultContent {
    let content = if result.content.len() > 2 {
        let first = result
            .content
            .first()
            .cloned()
            .unwrap_or_else(|| rmcp::model::Content::text(""));
        let last = result
            .content
            .last()
            .cloned()
            .unwrap_or_else(|| rmcp::model::Content::text(""));

        vec![
            first,
            rmcp::model::Content::text(truncated_hint.to_string()),
            last,
        ]
    } else {
        result.content.clone()
    };

    rmcp::model::ToolResultContent::new(&result.tool_use_id, content)
}

/// Prune tool results based on importance levels
#[allow(dead_code)]
pub(crate) fn prune_tools_with_importance(
    messages: &[SamplingMessage],
    get_importance: impl Fn(&str) -> crate::config::ToolImportance,
) -> Vec<SamplingMessage> {
    messages
        .iter()
        .map(|msg| {
            let SamplingContent::Single(SamplingMessageContent::ToolResult(
                result,
            )) = &msg.content
            else {
                return msg.clone();
            };

            let importance = get_importance(&result.tool_use_id);

            match importance {
                crate::config::ToolImportance::Critical => msg.clone(),
                crate::config::ToolImportance::High => {
                    let pruned = soft_prune_tool_result(result);
                    SamplingMessage {
                        role: msg.role.clone(),
                        content: SamplingContent::Single(
                            SamplingMessageContent::ToolResult(pruned),
                        ),
                        meta: msg.meta.clone(),
                    }
                }
                crate::config::ToolImportance::Normal => {
                    let pruned = hard_prune_tool_result(result);
                    SamplingMessage {
                        role: msg.role.clone(),
                        content: SamplingContent::Single(
                            SamplingMessageContent::ToolResult(pruned),
                        ),
                        meta: msg.meta.clone(),
                    }
                }
                crate::config::ToolImportance::Low => {
                    let cleared = clear_tool_result(result);
                    SamplingMessage {
                        role: msg.role.clone(),
                        content: SamplingContent::Single(
                            SamplingMessageContent::ToolResult(cleared),
                        ),
                        meta: msg.meta.clone(),
                    }
                }
            }
        })
        .collect()
}

/// Prune tool result with a custom hint, keeping first and last items
fn prune_tool_result_with_hint(
    result: &rmcp::model::ToolResultContent,
    hint: &str,
) -> rmcp::model::ToolResultContent {
    prune_tool_result_by_level(result, hint)
}

/// Soft pruning: keep first and last items, truncate middle
fn soft_prune_tool_result(
    result: &rmcp::model::ToolResultContent,
) -> rmcp::model::ToolResultContent {
    let truncated_hint =
        format!("\n... [{} items truncated] ...\n", result.content.len() - 2);
    prune_tool_result_with_hint(result, &truncated_hint)
}

/// Hard pruning: keep only first and last items with minimal context
fn hard_prune_tool_result(
    result: &rmcp::model::ToolResultContent,
) -> rmcp::model::ToolResultContent {
    prune_tool_result_with_hint(result, "\n[content truncated]\n")
}

/// Clear tool result: replace with placeholder
fn clear_tool_result(
    result: &rmcp::model::ToolResultContent,
) -> rmcp::model::ToolResultContent {
    rmcp::model::ToolResultContent::new(
        &result.tool_use_id,
        vec![rmcp::model::Content::text("[tool result cleared]")],
    )
}

/// Soft pruning for all tool results (legacy behavior)
#[allow(dead_code)]
pub(crate) fn soft_prune_all_tools(
    messages: &[SamplingMessage],
) -> Vec<SamplingMessage> {
    messages
        .iter()
        .map(|msg| {
            let SamplingContent::Single(SamplingMessageContent::ToolResult(
                result,
            )) = &msg.content
            else {
                return msg.clone();
            };

            SamplingMessage {
                role: msg.role.clone(),
                content: SamplingContent::Single(
                    SamplingMessageContent::ToolResult(soft_prune_tool_result(
                        result,
                    )),
                ),
                meta: msg.meta.clone(),
            }
        })
        .collect()
}

/// Hard clearing for non-critical tools
#[allow(dead_code)]
pub(crate) fn hard_clear_non_critical_tools(
    messages: &[SamplingMessage],
    is_critical: impl Fn(&str) -> bool,
) -> Vec<SamplingMessage> {
    messages
        .iter()
        .map(|msg| {
            let SamplingContent::Single(SamplingMessageContent::ToolResult(
                result,
            )) = &msg.content
            else {
                return msg.clone();
            };

            if is_critical(&result.tool_use_id) {
                msg.clone()
            } else {
                SamplingMessage {
                    role: msg.role.clone(),
                    content: SamplingContent::Single(
                        SamplingMessageContent::ToolResult(clear_tool_result(
                            result,
                        )),
                    ),
                    meta: msg.meta.clone(),
                }
            }
        })
        .collect()
}

/// Fix tool pairs in a message sequence by removing orphaned ToolResults or adding missing ToolUses
pub(crate) fn fix_tool_pairs(
    messages: &[SamplingMessage],
) -> Vec<SamplingMessage> {
    use std::collections::HashSet;

    let mut pending_tool_uses: HashSet<String> = HashSet::new();
    let mut result = Vec::with_capacity(messages.len());

    for msg in messages {
        let tool_use_ids = extract_tool_use_ids(msg);
        let tool_result_ids = extract_tool_result_ids(msg);

        match (tool_use_ids.is_empty(), tool_result_ids.is_empty()) {
            (false, _) => {
                pending_tool_uses.extend(tool_use_ids);
                result.push(msg.clone());
            }
            (true, false) => {
                if tool_result_ids
                    .iter()
                    .all(|id| pending_tool_uses.contains(id))
                {
                    for id in &tool_result_ids {
                        pending_tool_uses.remove(id);
                    }
                    result.push(msg.clone());
                }
            }
            _ => {
                result.push(msg.clone());
            }
        }
    }

    result
}

/// Micro-compact: replace old tool results with lightweight placeholders
///
/// This is similar to learn-claude-code s06's micro_compact:
/// - Keep only the last K tool results
/// - Replace older tool results with "[Previous: used {tool_name}]" placeholders
/// - This runs on every turn before LLM call, unlike full compaction
///
/// # Arguments
///
/// * `messages` - The conversation messages
/// * `keep_recent` - Number of recent tool results to keep (default: 3)
///
/// # Returns
///
/// Messages with old tool results replaced by placeholders
pub(crate) fn micro_compact(
    messages: &mut [SamplingMessage],
    keep_recent: usize,
) {
    let tool_result_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter_map(|(idx, msg)| {
            if let SamplingContent::Single(
                SamplingMessageContent::ToolResult(_),
            ) = &msg.content
            {
                Some(idx)
            } else {
                None
            }
        })
        .collect();

    if tool_result_indices.len() <= keep_recent {
        return;
    }

    for &msg_idx in tool_result_indices
        .iter()
        .take(tool_result_indices.len() - keep_recent)
    {
        if let Some(msg) = messages.get_mut(msg_idx)
            && let SamplingContent::Single(SamplingMessageContent::ToolResult(
                result,
            )) = &mut msg.content
        {
            result.content =
                vec![rmcp::model::Content::text("[cleared]".to_string())];
        }
    }
}

// Critical tool result patterns for extract_critical_tool_results
const CRITICAL_READ_PREFIX: &str = "[read]";
const CRITICAL_FILE_PREFIX: &str = "File:";
const CRITICAL_CODE_BLOCK: &str = "```";
const CRITICAL_CONTENT_MIN_LENGTH: usize = 1000;

/// Extract critical tool results (e.g., read tool results with file content)
pub(crate) fn extract_critical_tool_results(
    messages: &[SamplingMessage],
) -> Vec<usize> {
    messages
        .iter()
        .enumerate()
        .filter_map(|(idx, msg)| {
            if let SamplingContent::Single(
                SamplingMessageContent::ToolResult(result),
            ) = &msg.content
            {
                let content_text: String = result
                    .content
                    .iter()
                    .filter_map(|c| c.as_text())
                    .map(|t| t.text.as_str())
                    .collect();

                let is_critical = content_text.contains(CRITICAL_READ_PREFIX)
                    || content_text.contains(CRITICAL_FILE_PREFIX)
                    || content_text.contains(CRITICAL_CODE_BLOCK)
                    || content_text.len() > CRITICAL_CONTENT_MIN_LENGTH;

                if is_critical { Some(idx) } else { None }
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use rmcp::model::{
        Content,
        RawTextContent,
        ToolResultContent,
        ToolUseContent,
    };

    use super::*;
    use crate::config::ToolImportance;

    // =============================================================================
    // Test Helpers
    // =============================================================================

    fn create_user_text_message(role: Role, text: &str) -> SamplingMessage {
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

    fn create_tool_use_message(
        role: Role,
        id: &str,
        name: &str,
        args: serde_json::Value,
    ) -> SamplingMessage {
        SamplingMessage {
            role,
            content: SamplingContent::Single(SamplingMessageContent::ToolUse(
                ToolUseContent::new(
                    id,
                    name,
                    args.as_object().cloned().unwrap_or_default(),
                ),
            )),
            meta: None,
        }
    }

    fn create_tool_result_message(
        role: Role,
        tool_use_id: &str,
        result_text: &str,
    ) -> SamplingMessage {
        SamplingMessage {
            role,
            content: SamplingContent::Single(
                SamplingMessageContent::ToolResult(ToolResultContent::new(
                    tool_use_id,
                    vec![Content::text(result_text.to_string())],
                )),
            ),
            meta: None,
        }
    }

    fn create_tool_result_message_multi(
        role: Role,
        tool_use_id: &str,
        items: Vec<&str>,
    ) -> SamplingMessage {
        let content: Vec<Content> =
            items.iter().map(|s| Content::text(s.to_string())).collect();
        SamplingMessage {
            role,
            content: SamplingContent::Single(
                SamplingMessageContent::ToolResult(ToolResultContent::new(
                    tool_use_id,
                    content,
                )),
            ),
            meta: None,
        }
    }

    fn _create_test_tool_result(id: &str, items: usize) -> SamplingMessage {
        let content: Vec<rmcp::model::Content> = (0..items)
            .map(|i| rmcp::model::Content::text(format!("Item {i}")))
            .collect();

        SamplingMessage {
            role: rmcp::model::Role::User,
            content: SamplingContent::Single(
                SamplingMessageContent::ToolResult(
                    rmcp::model::ToolResultContent::new(id, content),
                ),
            ),
            meta: None,
        }
    }

    #[test]
    fn test_fix_tool_pairs_with_single_content() {
        // Test fix_tool_pairs with Single content
        let messages = vec![
            SamplingMessage {
                role: rmcp::model::Role::Assistant,
                content: SamplingContent::Single(
                    SamplingMessageContent::ToolUse(
                        rmcp::model::ToolUseContent::new(
                            "tool-1",
                            "test_tool",
                            serde_json::json!({})
                                .as_object()
                                .cloned()
                                .unwrap_or_default(),
                        ),
                    ),
                ),
                meta: None,
            },
            SamplingMessage {
                role: rmcp::model::Role::User,
                content: SamplingContent::Single(
                    SamplingMessageContent::ToolResult(
                        rmcp::model::ToolResultContent::new(
                            "tool-1",
                            vec![rmcp::model::Content::text("result")],
                        ),
                    ),
                ),
                meta: None,
            },
        ];

        let fixed = fix_tool_pairs(&messages);
        assert_eq!(fixed.len(), 2);
    }

    #[test]
    fn test_fix_tool_pairs_with_multiple_content() {
        // Test fix_tool_pairs with Multiple content (assistant message with multiple tool_uses)
        let messages = vec![
            SamplingMessage {
                role: rmcp::model::Role::Assistant,
                content: SamplingContent::Multiple(vec![
                    SamplingMessageContent::ToolUse(
                        rmcp::model::ToolUseContent::new(
                            "tool-1",
                            "test_tool_1",
                            serde_json::json!({})
                                .as_object()
                                .cloned()
                                .unwrap_or_default(),
                        ),
                    ),
                    SamplingMessageContent::ToolUse(
                        rmcp::model::ToolUseContent::new(
                            "tool-2",
                            "test_tool_2",
                            serde_json::json!({})
                                .as_object()
                                .cloned()
                                .unwrap_or_default(),
                        ),
                    ),
                ]),
                meta: None,
            },
            SamplingMessage {
                role: rmcp::model::Role::User,
                content: SamplingContent::Single(
                    SamplingMessageContent::ToolResult(
                        rmcp::model::ToolResultContent::new(
                            "tool-1",
                            vec![rmcp::model::Content::text("result1")],
                        ),
                    ),
                ),
                meta: None,
            },
            SamplingMessage {
                role: rmcp::model::Role::User,
                content: SamplingContent::Single(
                    SamplingMessageContent::ToolResult(
                        rmcp::model::ToolResultContent::new(
                            "tool-2",
                            vec![rmcp::model::Content::text("result2")],
                        ),
                    ),
                ),
                meta: None,
            },
        ];

        let fixed = fix_tool_pairs(&messages);
        assert_eq!(fixed.len(), 3);
    }

    #[test]
    fn test_fix_tool_pairs_removes_orphaned_results() {
        // Test that orphaned ToolResults are removed
        let messages = vec![SamplingMessage {
            role: rmcp::model::Role::User,
            content: SamplingContent::Single(
                SamplingMessageContent::ToolResult(
                    rmcp::model::ToolResultContent::new(
                        "orphan-tool",
                        vec![rmcp::model::Content::text("result")],
                    ),
                ),
            ),
            meta: None,
        }];

        let fixed = fix_tool_pairs(&messages);
        assert_eq!(fixed.len(), 0);
    }

    #[test]
    fn test_fix_tool_pairs_with_mixed_content() {
        // Test with a mix of Single and Multiple content
        let messages = vec![
            SamplingMessage {
                role: rmcp::model::Role::Assistant,
                content: SamplingContent::Multiple(vec![
                    SamplingMessageContent::Text(rmcp::model::RawTextContent {
                        text: "I'll help you".to_string(),
                        meta: None,
                    }),
                    SamplingMessageContent::ToolUse(
                        rmcp::model::ToolUseContent::new(
                            "tool-1",
                            "test_tool",
                            serde_json::json!({})
                                .as_object()
                                .cloned()
                                .unwrap_or_default(),
                        ),
                    ),
                ]),
                meta: None,
            },
            SamplingMessage {
                role: rmcp::model::Role::User,
                content: SamplingContent::Single(
                    SamplingMessageContent::ToolResult(
                        rmcp::model::ToolResultContent::new(
                            "tool-1",
                            vec![rmcp::model::Content::text("result")],
                        ),
                    ),
                ),
                meta: None,
            },
        ];

        let fixed = fix_tool_pairs(&messages);
        assert_eq!(fixed.len(), 2);
    }

    // =============================================================================
    // is_user_text_message Tests
    // =============================================================================

    #[test]
    fn test_is_user_text_message_user_role_not_tool_result() {
        let msg = create_user_text_message(Role::User, "Hello world");
        assert!(is_user_text_message(&msg));
    }

    #[test]
    fn test_is_user_text_message_assistant_role() {
        let msg = create_user_text_message(Role::Assistant, "I will help");
        assert!(!is_user_text_message(&msg));
    }

    #[test]
    fn test_is_user_text_message_user_role_with_tool_result() {
        let msg = SamplingMessage {
            role: Role::User,
            content: SamplingContent::Single(
                SamplingMessageContent::ToolResult(ToolResultContent::new(
                    "tool-1",
                    vec![Content::text("result")],
                )),
            ),
            meta: None,
        };
        assert!(!is_user_text_message(&msg));
    }

    // =============================================================================
    // is_tool_use Tests
    // =============================================================================

    #[test]
    fn test_is_tool_use_true() {
        let msg = create_tool_use_message(
            Role::Assistant,
            "tool-1",
            "read_file",
            serde_json::json!({}),
        );
        assert!(is_tool_use(&msg));
    }

    #[test]
    fn test_is_tool_use_false_for_text() {
        let msg = create_user_text_message(Role::Assistant, "Just text");
        assert!(!is_tool_use(&msg));
    }

    #[test]
    fn test_is_tool_use_false_for_tool_result() {
        let msg = create_tool_result_message(Role::User, "tool-1", "result");
        assert!(!is_tool_use(&msg));
    }

    // =============================================================================
    // is_tool_result Tests
    // =============================================================================

    #[test]
    fn test_is_tool_result_true() {
        let msg = create_tool_result_message(Role::User, "tool-1", "result");
        assert!(is_tool_result(&msg));
    }

    #[test]
    fn test_is_tool_result_false_for_text() {
        let msg = create_user_text_message(Role::User, "Hello");
        assert!(!is_tool_result(&msg));
    }

    #[test]
    fn test_is_tool_result_false_for_tool_use() {
        let msg = create_tool_use_message(
            Role::Assistant,
            "tool-1",
            "read",
            serde_json::json!({}),
        );
        assert!(!is_tool_result(&msg));
    }

    // =============================================================================
    // get_tool_use_id Tests
    // =============================================================================

    #[test]
    fn test_get_tool_use_id_found() {
        let msg = create_tool_use_message(
            Role::Assistant,
            "tool-abc",
            "read",
            serde_json::json!({}),
        );
        assert_eq!(get_tool_use_id(&msg), Some("tool-abc".to_string()));
    }

    #[test]
    fn test_get_tool_use_id_not_found() {
        let msg = create_user_text_message(Role::User, "Hello");
        assert_eq!(get_tool_use_id(&msg), None);
    }

    // =============================================================================
    // get_tool_result_id Tests
    // =============================================================================

    #[test]
    fn test_get_tool_result_id_found() {
        let msg = create_tool_result_message(
            Role::User,
            "tool-xyz",
            "result content",
        );
        assert_eq!(get_tool_result_id(&msg), Some("tool-xyz".to_string()));
    }

    #[test]
    fn test_get_tool_result_id_not_found() {
        let msg = create_user_text_message(Role::User, "Hello");
        assert_eq!(get_tool_result_id(&msg), None);
    }

    #[test]
    fn test_get_tool_result_id_not_found_for_tool_use() {
        let msg = create_tool_use_message(
            Role::Assistant,
            "tool-1",
            "read",
            serde_json::json!({}),
        );
        assert_eq!(get_tool_result_id(&msg), None);
    }

    // =============================================================================
    // classify_messages Tests
    // =============================================================================

    #[test]
    fn test_classify_messages_user_text() {
        let messages = vec![create_user_text_message(Role::User, "Hello")];
        let classified = classify_messages(&messages);
        assert_eq!(classified, vec![(0, MessageClassification::UserText)]);
    }

    #[test]
    fn test_classify_messages_assistant_text() {
        let messages =
            vec![create_user_text_message(Role::Assistant, "I will help")];
        let classified = classify_messages(&messages);
        assert_eq!(classified, vec![(0, MessageClassification::AssistantText)]);
    }

    #[test]
    fn test_classify_messages_tool_use() {
        let messages = vec![create_tool_use_message(
            Role::Assistant,
            "tool-1",
            "read",
            serde_json::json!({}),
        )];
        let classified = classify_messages(&messages);
        assert_eq!(classified, vec![(0, MessageClassification::ToolUse)]);
    }

    #[test]
    fn test_classify_messages_tool_result() {
        let messages =
            vec![create_tool_result_message(Role::User, "tool-1", "result")];
        let classified = classify_messages(&messages);
        assert_eq!(classified, vec![(0, MessageClassification::ToolResult)]);
    }

    #[test]
    fn test_classify_messages_mixed() {
        let messages = vec![
            create_user_text_message(Role::User, "Hello"),
            create_tool_use_message(
                Role::Assistant,
                "tool-1",
                "read",
                serde_json::json!({}),
            ),
            create_tool_result_message(Role::User, "tool-1", "result"),
            create_user_text_message(Role::Assistant, "Got it"),
        ];
        let classified = classify_messages(&messages);
        assert_eq!(
            classified,
            vec![
                (0, MessageClassification::UserText),
                (1, MessageClassification::ToolUse),
                (2, MessageClassification::ToolResult),
                (3, MessageClassification::AssistantText),
            ]
        );
    }

    // =============================================================================
    // find_tool_use_for_result Tests
    // =============================================================================

    #[test]
    fn test_find_tool_use_for_result_found() {
        let messages = vec![
            create_tool_use_message(
                Role::Assistant,
                "tool-1",
                "read",
                serde_json::json!({}),
            ),
            create_tool_result_message(Role::User, "tool-1", "result"),
        ];
        assert_eq!(find_tool_use_for_result(&messages, 1), Some(0));
    }

    #[test]
    fn test_find_tool_use_for_result_not_found() {
        let messages =
            vec![create_tool_result_message(Role::User, "orphan", "result")];
        assert_eq!(find_tool_use_for_result(&messages, 0), None);
    }

    #[test]
    fn test_find_tool_use_for_result_out_of_bounds() {
        let messages = vec![create_user_text_message(Role::User, "Hello")];
        assert_eq!(find_tool_use_for_result(&messages, 5), None);
    }

    #[test]
    fn test_find_tool_use_for_result_multiple_tools() {
        let messages = vec![
            create_tool_use_message(
                Role::Assistant,
                "tool-1",
                "read",
                serde_json::json!({}),
            ),
            create_tool_use_message(
                Role::Assistant,
                "tool-2",
                "write",
                serde_json::json!({}),
            ),
            create_tool_result_message(Role::User, "tool-1", "result1"),
            create_tool_result_message(Role::User, "tool-2", "result2"),
        ];
        assert_eq!(find_tool_use_for_result(&messages, 2), Some(0));
        assert_eq!(find_tool_use_for_result(&messages, 3), Some(1));
    }

    // =============================================================================
    // find_result_for_tool_use Tests
    // =============================================================================

    #[test]
    fn test_find_result_for_tool_use_found() {
        let messages = vec![
            create_tool_use_message(
                Role::Assistant,
                "tool-1",
                "read",
                serde_json::json!({}),
            ),
            create_tool_result_message(Role::User, "tool-1", "result"),
        ];
        assert_eq!(find_result_for_tool_use(&messages, 0), Some(1));
    }

    #[test]
    fn test_find_result_for_tool_use_not_found() {
        let messages = vec![create_tool_use_message(
            Role::Assistant,
            "tool-1",
            "read",
            serde_json::json!({}),
        )];
        assert_eq!(find_result_for_tool_use(&messages, 0), None);
    }

    #[test]
    fn test_find_result_for_tool_use_out_of_bounds() {
        let messages = vec![create_user_text_message(Role::User, "Hello")];
        assert_eq!(find_result_for_tool_use(&messages, 5), None);
    }

    #[test]
    fn test_find_result_for_tool_use_multiple_tools() {
        let messages = vec![
            create_tool_use_message(
                Role::Assistant,
                "tool-1",
                "read",
                serde_json::json!({}),
            ),
            create_tool_use_message(
                Role::Assistant,
                "tool-2",
                "write",
                serde_json::json!({}),
            ),
            create_tool_result_message(Role::User, "tool-1", "result1"),
            create_tool_result_message(Role::User, "tool-2", "result2"),
        ];
        assert_eq!(find_result_for_tool_use(&messages, 0), Some(2));
        assert_eq!(find_result_for_tool_use(&messages, 1), Some(3));
    }

    // =============================================================================
    // extract_tool_use_ids Tests
    // =============================================================================

    #[test]
    fn test_extract_tool_use_ids_single() {
        let msg = create_tool_use_message(
            Role::Assistant,
            "tool-1",
            "read",
            serde_json::json!({}),
        );
        let ids = extract_tool_use_ids(&msg);
        assert_eq!(ids, vec!["tool-1"]);
    }

    #[test]
    fn test_extract_tool_use_ids_multiple() {
        let msg = SamplingMessage {
            role: Role::Assistant,
            content: SamplingContent::Multiple(vec![
                SamplingMessageContent::ToolUse(ToolUseContent::new(
                    "tool-1",
                    "read",
                    serde_json::json!({})
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                )),
                SamplingMessageContent::ToolUse(ToolUseContent::new(
                    "tool-2",
                    "write",
                    serde_json::json!({})
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                )),
            ]),
            meta: None,
        };
        let ids = extract_tool_use_ids(&msg);
        assert_eq!(ids, vec!["tool-1", "tool-2"]);
    }

    #[test]
    fn test_extract_tool_use_ids_none() {
        let msg = create_user_text_message(Role::User, "Hello");
        let ids = extract_tool_use_ids(&msg);
        assert!(ids.is_empty());
    }

    // =============================================================================
    // extract_tool_result_ids Tests
    // =============================================================================

    #[test]
    fn test_extract_tool_result_ids_single() {
        let msg = create_tool_result_message(Role::User, "tool-1", "result");
        let ids = extract_tool_result_ids(&msg);
        assert_eq!(ids, vec!["tool-1"]);
    }

    #[test]
    fn test_extract_tool_result_ids_multiple() {
        let msg = SamplingMessage {
            role: Role::User,
            content: SamplingContent::Multiple(vec![
                SamplingMessageContent::ToolResult(ToolResultContent::new(
                    "tool-1",
                    vec![Content::text("result1")],
                )),
                SamplingMessageContent::ToolResult(ToolResultContent::new(
                    "tool-2",
                    vec![Content::text("result2")],
                )),
            ]),
            meta: None,
        };
        let ids = extract_tool_result_ids(&msg);
        assert_eq!(ids, vec!["tool-1", "tool-2"]);
    }

    #[test]
    fn test_extract_tool_result_ids_none() {
        let msg = create_user_text_message(Role::User, "Hello");
        let ids = extract_tool_result_ids(&msg);
        assert!(ids.is_empty());
    }

    // =============================================================================
    // prune_tool_result_by_level Tests
    // =============================================================================

    #[test]
    fn test_prune_tool_result_by_level_more_than_two_items() {
        let result = ToolResultContent::new(
            "tool-1",
            vec![
                Content::text("Item 0"),
                Content::text("Item 1"),
                Content::text("Item 2"),
                Content::text("Item 3"),
            ],
        );
        let pruned = prune_tool_result_by_level(
            &result,
            "\n... [2 items truncated] ...\n",
        );
        assert_eq!(pruned.content.len(), 3);
        assert_eq!(pruned.content[0].as_text().unwrap().text, "Item 0");
        assert_eq!(
            pruned.content[1].as_text().unwrap().text,
            "\n... [2 items truncated] ...\n"
        );
        assert_eq!(pruned.content[2].as_text().unwrap().text, "Item 3");
    }

    #[test]
    fn test_prune_tool_result_by_level_two_items() {
        let result = ToolResultContent::new(
            "tool-1",
            vec![Content::text("Item 0"), Content::text("Item 1")],
        );
        let pruned = prune_tool_result_by_level(
            &result,
            "\n... [2 items truncated] ...\n",
        );
        assert_eq!(pruned.content.len(), 2);
        assert_eq!(pruned.content[0].as_text().unwrap().text, "Item 0");
        assert_eq!(pruned.content[1].as_text().unwrap().text, "Item 1");
    }

    #[test]
    fn test_prune_tool_result_by_level_one_item() {
        let result =
            ToolResultContent::new("tool-1", vec![Content::text("Only item")]);
        let pruned = prune_tool_result_by_level(
            &result,
            "\n... [2 items truncated] ...\n",
        );
        assert_eq!(pruned.content.len(), 1);
        assert_eq!(pruned.content[0].as_text().unwrap().text, "Only item");
    }

    #[test]
    fn test_prune_tool_result_by_level_empty() {
        let result = ToolResultContent::new("tool-1", vec![]);
        let pruned = prune_tool_result_by_level(&result, "hint");
        assert!(pruned.content.is_empty());
    }

    // =============================================================================
    // prune_tool_result_with_hint Tests
    // =============================================================================

    #[test]
    fn test_prune_tool_result_with_hint() {
        let result = ToolResultContent::new(
            "tool-1",
            vec![
                Content::text("First"),
                Content::text("Middle"),
                Content::text("Last"),
            ],
        );
        let pruned = prune_tool_result_with_hint(&result, "[truncated]");
        assert_eq!(pruned.content.len(), 3);
        assert_eq!(pruned.content[1].as_text().unwrap().text, "[truncated]");
    }

    // =============================================================================
    // soft_prune_tool_result Tests
    // =============================================================================

    #[test]
    fn test_soft_prune_tool_result() {
        let result = ToolResultContent::new(
            "tool-1",
            vec![
                Content::text("Item 0"),
                Content::text("Item 1"),
                Content::text("Item 2"),
                Content::text("Item 3"),
            ],
        );
        let pruned = soft_prune_tool_result(&result);
        assert_eq!(pruned.content.len(), 3);
        assert_eq!(pruned.content[0].as_text().unwrap().text, "Item 0");
        assert!(
            pruned.content[1]
                .as_text()
                .unwrap()
                .text
                .contains("2 items truncated")
        );
        assert_eq!(pruned.content[2].as_text().unwrap().text, "Item 3");
    }

    #[test]
    fn test_soft_prune_tool_result_two_items_unchanged() {
        let result = ToolResultContent::new(
            "tool-1",
            vec![Content::text("First"), Content::text("Last")],
        );
        let pruned = soft_prune_tool_result(&result);
        assert_eq!(pruned.content.len(), 2);
    }

    // =============================================================================
    // hard_prune_tool_result Tests
    // =============================================================================

    #[test]
    fn test_hard_prune_tool_result() {
        let result = ToolResultContent::new(
            "tool-1",
            vec![
                Content::text("Item 0"),
                Content::text("Item 1"),
                Content::text("Item 2"),
                Content::text("Item 3"),
            ],
        );
        let pruned = hard_prune_tool_result(&result);
        assert_eq!(pruned.content.len(), 3);
        assert_eq!(pruned.content[0].as_text().unwrap().text, "Item 0");
        assert_eq!(
            pruned.content[1].as_text().unwrap().text,
            "\n[content truncated]\n"
        );
        assert_eq!(pruned.content[2].as_text().unwrap().text, "Item 3");
    }

    // =============================================================================
    // clear_tool_result Tests
    // =============================================================================

    #[test]
    fn test_clear_tool_result() {
        let result = ToolResultContent::new(
            "tool-1",
            vec![Content::text("Original content")],
        );
        let cleared = clear_tool_result(&result);
        assert_eq!(cleared.content.len(), 1);
        assert_eq!(
            cleared.content[0].as_text().unwrap().text,
            "[tool result cleared]"
        );
        assert_eq!(cleared.tool_use_id, "tool-1");
    }

    // =============================================================================
    // prune_tools_with_importance Tests
    // =============================================================================

    #[test]
    fn test_prune_tools_with_importance_critical() {
        use crate::config::ToolImportance;
        let messages =
            vec![create_tool_result_message(Role::User, "tool-1", "result")];
        let importance = |_: &str| ToolImportance::Critical;
        let pruned = prune_tools_with_importance(&messages, importance);
        assert_eq!(pruned[0], messages[0]);
    }

    #[test]
    fn test_prune_tools_with_importance_high() {
        use crate::config::ToolImportance;
        // Need 3+ items for soft prune to add the truncated hint
        let messages = vec![create_tool_result_message_multi(
            Role::User,
            "tool-1",
            vec!["Item 0", "Item 1", "Item 2", "Item 3"],
        )];
        let importance = |_: &str| ToolImportance::High;
        let pruned = prune_tools_with_importance(&messages, importance);
        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &pruned[0].content
        {
            assert!(
                result.content[1]
                    .as_text()
                    .unwrap()
                    .text
                    .contains("truncated")
            );
        } else {
            panic!("Expected ToolResult");
        }
    }

    #[test]
    fn test_prune_tools_with_importance_normal() {
        use crate::config::ToolImportance;
        // Need 3+ items for hard prune to add the truncated hint
        let messages = vec![create_tool_result_message_multi(
            Role::User,
            "tool-1",
            vec!["Item 0", "Item 1", "Item 2", "Item 3"],
        )];
        let importance = |_: &str| ToolImportance::Normal;
        let pruned = prune_tools_with_importance(&messages, importance);
        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &pruned[0].content
        {
            assert_eq!(
                result.content[1].as_text().unwrap().text,
                "\n[content truncated]\n"
            );
        } else {
            panic!("Expected ToolResult");
        }
    }

    #[test]
    fn test_prune_tools_with_importance_low() {
        use crate::config::ToolImportance;
        let messages = vec![create_tool_result_message(
            Role::User,
            "tool-1",
            "Original content",
        )];
        let importance = |_: &str| ToolImportance::Low;
        let pruned = prune_tools_with_importance(&messages, importance);
        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &pruned[0].content
        {
            assert_eq!(
                result.content[0].as_text().unwrap().text,
                "[tool result cleared]"
            );
        } else {
            panic!("Expected ToolResult");
        }
    }

    #[test]
    fn test_prune_tools_with_importance_non_tool_message_unchanged() {
        use crate::config::ToolImportance;
        let messages = vec![create_user_text_message(Role::User, "Hello")];
        let importance = |_: &str| ToolImportance::Critical;
        let pruned = prune_tools_with_importance(&messages, importance);
        assert_eq!(pruned[0], messages[0]);
    }

    // =============================================================================
    // soft_prune_all_tools Tests
    // =============================================================================

    #[test]
    fn test_soft_prune_all_tools() {
        let messages = vec![
            create_tool_use_message(
                Role::Assistant,
                "tool-1",
                "read",
                serde_json::json!({}),
            ),
            create_tool_result_message_multi(
                Role::User,
                "tool-1",
                vec!["Item 0", "Item 1", "Item 2", "Item 3"],
            ),
        ];
        let pruned = soft_prune_all_tools(&messages);
        assert_eq!(pruned[0], messages[0]);
        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &pruned[1].content
        {
            assert!(
                result.content[1]
                    .as_text()
                    .unwrap()
                    .text
                    .contains("truncated")
            );
        } else {
            panic!("Expected ToolResult");
        }
    }

    #[test]
    fn test_soft_prune_all_tools_preserves_non_tools() {
        let messages = vec![
            create_user_text_message(Role::User, "Hello"),
            create_user_text_message(Role::Assistant, "Hi there"),
        ];
        let pruned = soft_prune_all_tools(&messages);
        assert_eq!(pruned, messages);
    }

    // =============================================================================
    // hard_clear_non_critical_tools Tests
    // =============================================================================

    #[test]
    fn test_hard_clear_non_critical_tools_critical_kept() {
        let messages = vec![create_tool_result_message(
            Role::User,
            "tool-critical",
            "important",
        )];
        let is_critical = |id: &str| id == "tool-critical";
        let cleared = hard_clear_non_critical_tools(&messages, is_critical);
        assert_eq!(cleared[0], messages[0]);
    }

    #[test]
    fn test_hard_clear_non_critical_tools_non_critical_cleared() {
        let messages = vec![create_tool_result_message(
            Role::User,
            "tool-normal",
            "content",
        )];
        let is_critical = |id: &str| id == "tool-critical";
        let cleared = hard_clear_non_critical_tools(&messages, is_critical);
        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &cleared[0].content
        {
            assert_eq!(
                result.content[0].as_text().unwrap().text,
                "[tool result cleared]"
            );
        } else {
            panic!("Expected ToolResult");
        }
    }

    #[test]
    fn test_hard_clear_non_critical_tools_preserves_non_tool() {
        let messages = vec![create_user_text_message(Role::User, "Hello")];
        let is_critical = |_: &str| false;
        let cleared = hard_clear_non_critical_tools(&messages, is_critical);
        assert_eq!(cleared[0], messages[0]);
    }

    // =============================================================================
    // micro_compact Tests
    // =============================================================================

    #[test]
    fn test_micro_compact_keeps_recent() {
        let mut messages = vec![
            create_tool_use_message(
                Role::Assistant,
                "tool-1",
                "read",
                serde_json::json!({}),
            ),
            create_tool_result_message(Role::User, "tool-1", "result1"),
            create_tool_use_message(
                Role::Assistant,
                "tool-2",
                "write",
                serde_json::json!({}),
            ),
            create_tool_result_message(Role::User, "tool-2", "result2"),
            create_tool_use_message(
                Role::Assistant,
                "tool-3",
                "delete",
                serde_json::json!({}),
            ),
            create_tool_result_message(Role::User, "tool-3", "result3"),
        ];
        // With keep_recent=2, tool results at indices [1,3,5] are candidates.
        // The implementation clears the FIRST (3-2)=1 items: index 1 gets cleared.
        // Indices 3 and 5 (the last 2) are kept.
        micro_compact(&mut messages, 2);
        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &messages[1].content
        {
            assert_eq!(result.content[0].as_text().unwrap().text, "[cleared]");
        } else {
            panic!("Expected ToolResult at index 1");
        }
        // Index 3 is kept (one of last 2), should still be result2
        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &messages[3].content
        {
            assert_eq!(result.content[0].as_text().unwrap().text, "result2");
        } else {
            panic!("Expected ToolResult at index 3");
        }
        // Index 5 is kept (last of last 2)
        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &messages[5].content
        {
            assert_eq!(result.content[0].as_text().unwrap().text, "result3");
        } else {
            panic!("Expected ToolResult at index 5");
        }
    }

    #[test]
    fn test_micro_compact_keeps_all_when_within_limit() {
        let mut messages = vec![
            create_tool_use_message(
                Role::Assistant,
                "tool-1",
                "read",
                serde_json::json!({}),
            ),
            create_tool_result_message(Role::User, "tool-1", "result1"),
            create_tool_use_message(
                Role::Assistant,
                "tool-2",
                "write",
                serde_json::json!({}),
            ),
            create_tool_result_message(Role::User, "tool-2", "result2"),
        ];
        micro_compact(&mut messages, 3);
        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &messages[1].content
        {
            assert_eq!(result.content[0].as_text().unwrap().text, "result1");
        } else {
            panic!("Expected ToolResult");
        }
    }

    #[test]
    fn test_micro_compact_empty_list() {
        let mut messages: Vec<SamplingMessage> = vec![];
        micro_compact(&mut messages, 2);
        assert!(messages.is_empty());
    }

    #[test]
    fn test_micro_compact_no_tool_results() {
        let mut messages = vec![
            create_user_text_message(Role::User, "Hello"),
            create_user_text_message(Role::Assistant, "Hi"),
        ];
        micro_compact(&mut messages, 2);
        assert_eq!(messages.len(), 2);
    }

    // =============================================================================
    // extract_critical_tool_results Tests
    // =============================================================================

    #[test]
    fn test_extract_critical_tool_results_read_prefix() {
        let messages = vec![create_tool_result_message(
            Role::User,
            "tool-1",
            "[read] file.txt: content here",
        )];
        let critical = extract_critical_tool_results(&messages);
        assert_eq!(critical, vec![0]);
    }

    #[test]
    fn test_extract_critical_tool_results_file_prefix() {
        let messages = vec![create_tool_result_message(
            Role::User,
            "tool-1",
            "File: /path/to/file.txt\ncontent",
        )];
        let critical = extract_critical_tool_results(&messages);
        assert_eq!(critical, vec![0]);
    }

    #[test]
    fn test_extract_critical_tool_results_code_block() {
        let messages = vec![create_tool_result_message(
            Role::User,
            "tool-1",
            "```\nlet x = 1;\n```",
        )];
        let critical = extract_critical_tool_results(&messages);
        assert_eq!(critical, vec![0]);
    }

    #[test]
    fn test_extract_critical_tool_results_long_content() {
        let long_text = "a".repeat(1001);
        let messages =
            vec![create_tool_result_message(Role::User, "tool-1", &long_text)];
        let critical = extract_critical_tool_results(&messages);
        assert_eq!(critical, vec![0]);
    }

    #[test]
    fn test_extract_critical_tool_results_non_critical() {
        let messages = vec![create_tool_result_message(
            Role::User,
            "tool-1",
            "simple result",
        )];
        let critical = extract_critical_tool_results(&messages);
        assert!(critical.is_empty());
    }

    #[test]
    fn test_extract_critical_tool_results_non_tool_message() {
        let messages = vec![
            create_user_text_message(Role::User, "Hello"),
            create_user_text_message(Role::Assistant, "Hi"),
        ];
        let critical = extract_critical_tool_results(&messages);
        assert!(critical.is_empty());
    }

    #[test]
    fn test_extract_critical_tool_results_mixed() {
        let messages = vec![
            create_tool_result_message(Role::User, "tool-1", "simple result"),
            create_tool_result_message(
                Role::User,
                "tool-2",
                "[read] file.txt: content",
            ),
            create_tool_result_message(Role::User, "tool-3", "another simple"),
        ];
        let critical = extract_critical_tool_results(&messages);
        assert_eq!(critical, vec![1]);
    }

    // =============================================================================
    // extract_critical_tool_results edge cases
    // =============================================================================

    #[test]
    fn test_extract_critical_tool_results_empty_content() {
        let messages =
            vec![create_tool_result_message(Role::User, "tool-1", "")];
        let critical = extract_critical_tool_results(&messages);
        assert!(critical.is_empty());
    }

    #[test]
    fn test_extract_critical_tool_results_file_prefix_case_sensitive() {
        // File: prefix should be case-sensitive
        let messages = vec![create_tool_result_message(
            Role::User,
            "tool-1",
            "file: some content", // lowercase
        )];
        let critical = extract_critical_tool_results(&messages);
        assert!(critical.is_empty());
    }

    #[test]
    fn test_extract_critical_tool_results_multiple_patterns() {
        let messages = vec![create_tool_result_message(
            Role::User,
            "tool-1",
            "[read] File: result.txt\n```\ncode\n```",
        )];
        let critical = extract_critical_tool_results(&messages);
        assert_eq!(critical, vec![0]);
    }

    #[test]
    fn test_extract_critical_tool_results_boundary_length() {
        // Exactly CRITICAL_CONTENT_MIN_LENGTH (1000) should NOT be critical
        let messages = vec![create_tool_result_message(
            Role::User,
            "tool-1",
            &"a".repeat(1000),
        )];
        let critical = extract_critical_tool_results(&messages);
        assert!(critical.is_empty());

        // 1001 should be critical
        let messages = vec![create_tool_result_message(
            Role::User,
            "tool-1",
            &"a".repeat(1001),
        )];
        let critical = extract_critical_tool_results(&messages);
        assert_eq!(critical, vec![0]);
    }

    // =============================================================================
    // fix_tool_pairs edge cases
    // =============================================================================

    #[test]
    fn test_fix_tool_pairs_empty() {
        let messages: Vec<SamplingMessage> = vec![];
        let fixed = fix_tool_pairs(&messages);
        assert!(fixed.is_empty());
    }

    #[test]
    fn test_fix_tool_pairs_single_tool_use() {
        let messages = vec![create_tool_use_message(
            Role::Assistant,
            "tool-1",
            "read",
            serde_json::json!({}),
        )];
        let fixed = fix_tool_pairs(&messages);
        // Single tool use is kept
        assert_eq!(fixed.len(), 1);
    }

    #[test]
    fn test_fix_tool_pairs_duplicate_tool_use_ids() {
        // Two tool uses with same ID, one result
        let messages = vec![
            create_tool_use_message(
                Role::Assistant,
                "tool-1",
                "read",
                serde_json::json!({}),
            ),
            create_tool_use_message(
                Role::Assistant,
                "tool-1",
                "write",
                serde_json::json!({}),
            ),
            create_tool_result_message(Role::User, "tool-1", "result"),
        ];
        let fixed = fix_tool_pairs(&messages);
        // Result should be kept since it matches pending tool
        assert_eq!(fixed.len(), 3);
    }

    #[test]
    fn test_fix_tool_pairs_multiple_results_same_id() {
        // Tool use followed by multiple results for same ID
        // Only the first result is kept because after it's consumed, pending is empty
        let messages = vec![
            create_tool_use_message(
                Role::Assistant,
                "tool-1",
                "read",
                serde_json::json!({}),
            ),
            create_tool_result_message(Role::User, "tool-1", "result1"),
            create_tool_result_message(Role::User, "tool-1", "result2"),
        ];
        let fixed = fix_tool_pairs(&messages);
        // First result kept (pending still has tool-1), second discarded (pending empty)
        assert_eq!(fixed.len(), 2);
    }

    #[test]
    fn test_fix_tool_pairs_result_before_tool() {
        // Orphan result that appears before its tool use
        let messages = vec![
            create_tool_result_message(Role::User, "tool-1", "orphan result"),
            create_tool_use_message(
                Role::Assistant,
                "tool-1",
                "read",
                serde_json::json!({}),
            ),
            create_tool_result_message(Role::User, "tool-1", "valid result"),
        ];
        let fixed = fix_tool_pairs(&messages);
        // First orphan result removed, others kept
        assert_eq!(fixed.len(), 2);
    }

    // =============================================================================
    // micro_compact edge cases
    // =============================================================================

    #[test]
    fn test_micro_compact_with_single_tool_result() {
        let mut messages = vec![
            create_tool_use_message(
                Role::Assistant,
                "tool-1",
                "read",
                serde_json::json!({}),
            ),
            create_tool_result_message(Role::User, "tool-1", "result1"),
        ];
        micro_compact(&mut messages, 1);
        // With keep_recent=1 and only 1 result, nothing should be cleared
        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &messages[1].content
        {
            assert_eq!(result.content[0].as_text().unwrap().text, "result1");
        } else {
            panic!("Expected ToolResult");
        }
    }

    #[test]
    fn test_micro_compact_keeps_all_when_equal_to_keep_recent() {
        let mut messages = vec![
            create_tool_use_message(
                Role::Assistant,
                "tool-1",
                "read",
                serde_json::json!({}),
            ),
            create_tool_result_message(Role::User, "tool-1", "result1"),
            create_tool_use_message(
                Role::Assistant,
                "tool-2",
                "write",
                serde_json::json!({}),
            ),
            create_tool_result_message(Role::User, "tool-2", "result2"),
        ];
        micro_compact(&mut messages, 2);
        // With keep_recent=2 and exactly 2 results, nothing should be cleared
        let result_count = messages
            .iter()
            .filter(|m| {
                matches!(
                    &m.content,
                    SamplingContent::Single(
                        SamplingMessageContent::ToolResult(_)
                    )
                )
            })
            .count();
        assert_eq!(result_count, 2);
    }

    // =============================================================================
    // MessageClassification edge cases
    // =============================================================================

    #[test]
    fn test_classify_messages_empty() {
        let messages: Vec<SamplingMessage> = vec![];
        let classified = classify_messages(&messages);
        assert!(classified.is_empty());
    }

    #[test]
    fn test_classify_messages_single_message() {
        let messages = vec![create_user_text_message(Role::User, "Hello")];
        let classified = classify_messages(&messages);
        assert_eq!(classified.len(), 1);
        assert_eq!(classified[0].0, 0);
        assert_eq!(classified[0].1, MessageClassification::UserText);
    }

    #[test]
    fn test_classify_messages_system_role() {
        // System role is not User or Assistant, so it should be Other
        let msg = SamplingMessage {
            role: Role::Assistant,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: "Assistant text".to_string(),
                    meta: None,
                },
            )),
            meta: None,
        };
        let messages = vec![msg];
        let classified = classify_messages(&messages);
        // Assistant role with text should be classified as AssistantText
        assert_eq!(classified[0].1, MessageClassification::AssistantText);
    }

    // =============================================================================
    // Tool ID extraction edge cases
    // =============================================================================

    #[test]
    fn test_get_tool_use_id_multiple_tool_uses() {
        let msg = SamplingMessage {
            role: Role::Assistant,
            content: SamplingContent::Multiple(vec![
                SamplingMessageContent::ToolUse(ToolUseContent::new(
                    "tool-1",
                    "read",
                    serde_json::json!({})
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                )),
                SamplingMessageContent::ToolUse(ToolUseContent::new(
                    "tool-2",
                    "write",
                    serde_json::json!({})
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                )),
            ]),
            meta: None,
        };
        // Should return the first tool use ID
        assert_eq!(get_tool_use_id(&msg), Some("tool-1".to_string()));
    }

    #[test]
    fn test_get_tool_result_id_multiple_tool_results() {
        let msg = SamplingMessage {
            role: Role::User,
            content: SamplingContent::Multiple(vec![
                SamplingMessageContent::ToolResult(ToolResultContent::new(
                    "tool-1",
                    vec![Content::text("result1")],
                )),
                SamplingMessageContent::ToolResult(ToolResultContent::new(
                    "tool-2",
                    vec![Content::text("result2")],
                )),
            ]),
            meta: None,
        };
        // Should return the first tool result ID
        assert_eq!(get_tool_result_id(&msg), Some("tool-1".to_string()));
    }

    // =============================================================================
    // Tool pair matching edge cases
    // =============================================================================

    #[test]
    fn test_find_tool_use_for_result_no_matching_tool() {
        let messages = vec![
            create_tool_use_message(
                Role::Assistant,
                "tool-1",
                "read",
                serde_json::json!({}),
            ),
            create_tool_result_message(Role::User, "tool-2", "result"), // Different ID
        ];
        assert_eq!(find_tool_use_for_result(&messages, 1), None);
    }

    #[test]
    fn test_find_result_for_tool_use_no_matching_result() {
        let messages = vec![
            create_tool_use_message(
                Role::Assistant,
                "tool-1",
                "read",
                serde_json::json!({}),
            ),
            create_tool_use_message(
                Role::Assistant,
                "tool-2",
                "write",
                serde_json::json!({}),
            ),
        ];
        assert_eq!(find_result_for_tool_use(&messages, 0), None);
        assert_eq!(find_result_for_tool_use(&messages, 1), None);
    }

    #[test]
    fn test_find_tool_use_for_result_with_multiple_tools() {
        let messages = vec![
            create_tool_use_message(
                Role::Assistant,
                "tool-1",
                "read",
                serde_json::json!({}),
            ),
            create_tool_use_message(
                Role::Assistant,
                "tool-2",
                "write",
                serde_json::json!({}),
            ),
            create_tool_result_message(Role::User, "tool-2", "result2"),
        ];
        // Should find tool-2 for result at index 2
        assert_eq!(find_tool_use_for_result(&messages, 2), Some(1));
    }

    // =============================================================================
    // prune_tool_result_by_level additional edge cases
    // =============================================================================

    #[test]
    fn test_prune_tool_result_by_level_preserves_order() {
        let result = ToolResultContent::new(
            "tool-1",
            vec![
                Content::text("First"),
                Content::text("Second"),
                Content::text("Third"),
                Content::text("Fourth"),
                Content::text("Fifth"),
            ],
        );
        let pruned = prune_tool_result_by_level(
            &result,
            "\n... [3 items truncated] ...\n",
        );
        assert_eq!(pruned.content.len(), 3);
        assert_eq!(pruned.content[0].as_text().unwrap().text, "First");
        assert_eq!(
            pruned.content[1].as_text().unwrap().text,
            "\n... [3 items truncated] ...\n"
        );
        assert_eq!(pruned.content[2].as_text().unwrap().text, "Fifth");
    }

    // =============================================================================
    // prune_tools_with_importance additional edge cases
    // =============================================================================

    #[test]
    fn test_prune_tools_with_importance_all_levels() {
        // Use multi-item messages to avoid underflow in soft_prune_tool_result
        let messages = vec![
            create_tool_result_message_multi(
                Role::User,
                "tool-critical",
                vec!["critical data"],
            ),
            create_tool_result_message_multi(
                Role::User,
                "tool-high",
                vec!["item1", "item2", "item3", "item4"],
            ),
            create_tool_result_message_multi(
                Role::User,
                "tool-normal",
                vec!["item1", "item2", "item3", "item4"],
            ),
            create_tool_result_message_multi(
                Role::User,
                "tool-low",
                vec!["low data"],
            ),
        ];
        let importance = |name: &str| match name {
            "tool-critical" => ToolImportance::Critical,
            "tool-high" => ToolImportance::High,
            "tool-normal" => ToolImportance::Normal,
            "tool-low" => ToolImportance::Low,
            _ => ToolImportance::Normal,
        };
        let pruned = prune_tools_with_importance(&messages, importance);

        // Critical - unchanged
        if let SamplingContent::Single(SamplingMessageContent::ToolResult(r)) =
            &pruned[0].content
        {
            assert_eq!(r.content[0].as_text().unwrap().text, "critical data");
        }

        // High - soft pruned (keeps first and last with truncated hint)
        if let SamplingContent::Single(SamplingMessageContent::ToolResult(r)) =
            &pruned[1].content
        {
            assert_eq!(r.content.len(), 3);
        }

        // Normal - hard pruned (keeps first and last with [content truncated])
        if let SamplingContent::Single(SamplingMessageContent::ToolResult(r)) =
            &pruned[2].content
        {
            assert_eq!(r.content.len(), 3);
            assert_eq!(
                r.content[1].as_text().unwrap().text,
                "\n[content truncated]\n"
            );
        }

        // Low - cleared
        if let SamplingContent::Single(SamplingMessageContent::ToolResult(r)) =
            &pruned[3].content
        {
            assert_eq!(
                r.content[0].as_text().unwrap().text,
                "[tool result cleared]"
            );
        }
    }

    // =============================================================================
    // extract_tool_use_ids and extract_tool_result_ids edge cases
    // =============================================================================

    #[test]
    fn test_extract_tool_use_ids_empty_message() {
        let msg = SamplingMessage {
            role: Role::Assistant,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: "No tool here".to_string(),
                    meta: None,
                },
            )),
            meta: None,
        };
        let ids = extract_tool_use_ids(&msg);
        assert!(ids.is_empty());
    }

    #[test]
    fn test_extract_tool_result_ids_empty_message() {
        let msg = SamplingMessage {
            role: Role::User,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                RawTextContent {
                    text: "No result here".to_string(),
                    meta: None,
                },
            )),
            meta: None,
        };
        let ids = extract_tool_result_ids(&msg);
        assert!(ids.is_empty());
    }
}
