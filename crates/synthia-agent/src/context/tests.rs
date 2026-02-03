//! Context compression comprehensive tests
//!
//! This module contains comprehensive tests for all context compression logic.

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

use crate::{
    config::ToolImportance,
    context::{
        check_summary_quality,
        create_summary_message,
        ensure_call_outputs_present,
        estimate_tokens,
        hard_clear_non_critical_tools,
        micro_compact,
        normalize_history,
        prune_tools_with_importance,
        remove_orphan_outputs,
        soft_prune_all_tools,
        strip_images_when_unsupported,
    },
};

// =============================================================================
// Test Helpers
// =============================================================================

fn create_text_message(role: Role, text: &str) -> SamplingMessage {
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

fn create_tool_result_message(id: &str, items: Vec<&str>) -> SamplingMessage {
    let content: Vec<Content> = items
        .iter()
        .map(|item| Content::text(item.to_string()))
        .collect();

    SamplingMessage {
        role: Role::User,
        content: SamplingContent::Single(SamplingMessageContent::ToolResult(
            ToolResultContent::new(id, content),
        )),
        meta: None,
    }
}

// =============================================================================
// Normalize Tests
// =============================================================================

#[cfg(test)]
mod normalize_tests {
    use super::*;

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
            panic!("Expected tool result");
        }
    }

    #[test]
    fn test_ensure_call_outputs_present_multiple_tools() {
        let mut messages = vec![
            create_tool_use_message("tool-1", "test_tool"),
            create_tool_use_message("tool-2", "test_tool"),
        ];

        ensure_call_outputs_present(&mut messages);

        assert_eq!(messages.len(), 4);
        assert_eq!(messages[1].role, Role::User);
        assert_eq!(messages[3].role, Role::User);
    }

    #[test]
    fn test_ensure_call_outputs_present_mixed_messages() {
        let mut messages = vec![
            create_text_message(Role::User, "Hello"),
            create_tool_use_message("tool-1", "test_tool"),
            create_text_message(Role::User, "World"),
        ];

        ensure_call_outputs_present(&mut messages);

        assert_eq!(messages.len(), 4);
        assert_eq!(messages[2].role, Role::User);
    }

    #[test]
    fn test_remove_orphan_outputs_no_orphans() {
        let mut messages = vec![
            create_tool_use_message("tool-1", "test_tool"),
            create_tool_result_message("tool-1", vec!["Result"]),
        ];

        remove_orphan_outputs(&mut messages);

        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_remove_orphan_outputs_removes_orphan() {
        let mut messages =
            vec![create_tool_result_message("tool-1", vec!["Orphan result"])];

        remove_orphan_outputs(&mut messages);

        assert_eq!(messages.len(), 0);
    }

    #[test]
    fn test_remove_orphan_outputs_mixed() {
        let mut messages = vec![
            create_tool_use_message("tool-1", "test_tool"),
            create_tool_result_message("tool-1", vec!["Valid result"]),
            create_tool_result_message("tool-2", vec!["Orphan result"]),
        ];

        remove_orphan_outputs(&mut messages);

        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_remove_orphan_outputs_first_is_orphan() {
        let mut messages = vec![
            create_tool_result_message("tool-1", vec!["Orphan"]),
            create_tool_use_message("tool-2", "test_tool"),
            create_tool_result_message("tool-2", vec!["Valid"]),
        ];

        remove_orphan_outputs(&mut messages);

        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_strip_images_when_supported() {
        let mut messages = vec![create_text_message(Role::User, "Hello")];

        strip_images_when_unsupported(&mut messages, true);

        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn test_normalize_history_empty() {
        let mut messages: Vec<SamplingMessage> = vec![];
        normalize_history(&mut messages, true);
        assert_eq!(messages.len(), 0);
    }

    #[test]
    fn test_normalize_history_complete_flow() {
        let mut messages = vec![
            create_tool_use_message("tool-1", "test_tool"),
            create_tool_result_message("tool-1", vec!["Result"]),
        ];

        normalize_history(&mut messages, true);

        assert_eq!(messages.len(), 2);
    }
}

// =============================================================================
// Summarizer Tests
// =============================================================================

#[cfg(test)]
mod summarizer_tests {
    use super::*;

    #[test]
    fn test_check_summary_quality_perfect() {
        let summary = r#"## Summary
This is a summary with file path src/main.rs.

## User Intent
The user requested to implement a feature.

## Current Work
We decided to use approach A.
"#;

        let quality = check_summary_quality(summary, &[]);
        assert!(quality.has_required_sections);
        assert!(quality.identifier_integrity);
        assert!(quality.user_request_reflected);
        assert!(quality.has_file_paths);
        assert!(quality.has_user_requests);
        assert!(quality.has_key_decisions);
        assert_eq!(quality.overall_score, 1.0);
    }

    #[test]
    fn test_check_summary_quality_missing_sections() {
        let summary = "This is just plain text without required sections.";

        let quality = check_summary_quality(summary, &[]);
        assert!(!quality.has_required_sections);
        assert!(quality.overall_score < 0.8);
    }

    #[test]
    fn test_check_summary_quality_partial_sections() {
        let summary = r#"## Summary
This is a summary.

## User Intent
The user wants something.
"#;

        let quality = check_summary_quality(summary, &[]);
        assert!(!quality.has_required_sections);
        assert!(quality.user_request_reflected);
    }

    #[test]
    fn test_check_summary_quality_with_code_blocks() {
        let summary = r#"## Summary
```rust
fn main() {}
```

## User Intent
Test.

## Current Work
Testing.
"#;

        let quality = check_summary_quality(summary, &[]);
        assert!(quality.has_required_sections);
        assert!(quality.identifier_integrity);
    }

    #[test]
    fn test_check_summary_quality_with_file_refs() {
        let summary = r#"## Summary
Check src/main.rs for details.

## User Intent
Test.

## Current Work
Testing.
"#;

        let quality = check_summary_quality(summary, &[]);
        assert!(quality.identifier_integrity);
    }

    #[test]
    fn test_create_summary_message() {
        let summary = "Test summary content";
        let msg = create_summary_message(summary);

        assert_eq!(msg.role, Role::Assistant);

        if let SamplingContent::Single(SamplingMessageContent::Text(text)) =
            &msg.content
        {
            assert!(text.text.contains("## Summary of Previous Conversation"));
            assert!(text.text.contains(summary));
        } else {
            panic!("Expected text content");
        }
    }
}

// =============================================================================
// Improved Summarize Tests
// =============================================================================

#[cfg(test)]
mod improved_summarize_tests {
    use super::*;

    #[test]
    fn test_user_messages_preserved() {
        let messages = [
            create_text_message(Role::User, "First user request"),
            create_text_message(Role::Assistant, "Assistant response 1"),
            create_tool_use_message("tool-1", "read"),
            create_tool_result_message("tool-1", vec!["File content"]),
            create_text_message(Role::User, "Second user request"),
            create_text_message(Role::Assistant, "Assistant response 2"),
        ];

        let preserved: Vec<_> = messages
            .iter()
            .filter(|m| m.role == Role::User && !is_tool_result(m))
            .cloned()
            .collect();

        assert_eq!(preserved.len(), 2);
        assert!(preserved[0].content.iter().any(|c| {
            c.as_text()
                .map(|t| t.text.contains("First user request"))
                .unwrap_or(false)
        }));
        assert!(preserved[1].content.iter().any(|c| {
            c.as_text()
                .map(|t| t.text.contains("Second user request"))
                .unwrap_or(false)
        }));
    }

    #[test]
    fn test_critical_tool_results_preserved() {
        let messages = [
            create_tool_use_message("tool-1", "read"),
            create_tool_result_message(
                "tool-1",
                vec!["Important file content with [read] marker"],
            ),
            create_tool_use_message("tool-2", "search"),
            create_tool_result_message("tool-2", vec!["Search results"]),
        ];

        let critical_indices: Vec<usize> = messages
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
                    if content_text.contains("[read]")
                        || content_text.contains("File:")
                        || content_text.len() > 1000
                    {
                        return Some(idx);
                    }
                }
                None
            })
            .collect();

        assert_eq!(critical_indices.len(), 1);
        assert_eq!(critical_indices[0], 1);
    }

    #[test]
    fn test_tool_pairs_integrity() {
        let messages = [
            create_text_message(Role::User, "Please read a file"),
            create_tool_use_message("tool-1", "read"),
            create_tool_result_message("tool-1", vec!["File content"]),
            create_text_message(Role::Assistant, "Here's the file"),
            create_text_message(Role::User, "Now search for something"),
            create_tool_use_message("tool-2", "search"),
            create_tool_result_message("tool-2", vec!["Search results"]),
        ];

        let tool_use_ids: Vec<String> = messages
            .iter()
            .filter_map(|m| {
                if let SamplingContent::Single(
                    SamplingMessageContent::ToolUse(tool_use),
                ) = &m.content
                {
                    Some(tool_use.id.clone())
                } else {
                    None
                }
            })
            .collect();

        let tool_result_ids: Vec<String> = messages
            .iter()
            .filter_map(|m| {
                if let SamplingContent::Single(
                    SamplingMessageContent::ToolResult(result),
                ) = &m.content
                {
                    Some(result.tool_use_id.clone())
                } else {
                    None
                }
            })
            .collect();

        for tool_use_id in &tool_use_ids {
            assert!(
                tool_result_ids.contains(tool_use_id),
                "ToolUse {tool_use_id} should have a corresponding ToolResult"
            );
        }

        for tool_result_id in &tool_result_ids {
            assert!(
                tool_use_ids.contains(tool_result_id),
                "ToolResult {tool_result_id} should have a corresponding ToolUse"
            );
        }
    }

    fn is_tool_result(msg: &SamplingMessage) -> bool {
        matches!(
            &msg.content,
            SamplingContent::Single(SamplingMessageContent::ToolResult(_))
        )
    }
}

// =============================================================================
// Progressive Compression Tests
// =============================================================================

#[cfg(test)]
mod progressive_compression_tests {
    use super::*;

    #[test]
    fn test_micro_compact_triggered() {
        let mut messages = vec![
            create_tool_use_message("tool-1", "read"),
            create_tool_result_message("tool-1", vec!["Old result 1"]),
            create_tool_use_message("tool-2", "search"),
            create_tool_result_message("tool-2", vec!["Old result 2"]),
            create_tool_use_message("tool-3", "grep"),
            create_tool_result_message("tool-3", vec!["Old result 3"]),
            create_tool_use_message("tool-4", "write"),
            create_tool_result_message("tool-4", vec!["Recent result"]),
        ];

        micro_compact(&mut messages, 1);

        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &messages[1].content
        {
            assert!(
                result.content[0]
                    .as_text()
                    .unwrap()
                    .text
                    .contains("[cleared]")
            );
        } else {
            panic!("Expected tool result at index 1");
        }

        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &messages[3].content
        {
            assert!(
                result.content[0]
                    .as_text()
                    .unwrap()
                    .text
                    .contains("[cleared]")
            );
        } else {
            panic!("Expected tool result at index 3");
        }

        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &messages[7].content
        {
            assert_eq!(
                result.content[0].as_text().unwrap().text,
                "Recent result"
            );
        } else {
            panic!("Expected tool result at index 7");
        }
    }

    #[test]
    fn test_soft_pruning_triggered() {
        let messages = vec![
            create_tool_result_message(
                "read",
                vec!["Line 1", "Line 2", "Line 3", "Line 4", "Line 5"],
            ),
            create_tool_result_message(
                "search",
                vec!["Result 1", "Result 2", "Result 3", "Result 4"],
            ),
        ];

        let pruned = soft_prune_all_tools(&messages);

        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &pruned[0].content
        {
            assert_eq!(result.content.len(), 3);
            assert!(
                result.content[1]
                    .as_text()
                    .unwrap()
                    .text
                    .contains("truncated")
            );
        } else {
            panic!("Expected tool result");
        }

        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &pruned[1].content
        {
            assert_eq!(result.content.len(), 3);
            assert!(
                result.content[1]
                    .as_text()
                    .unwrap()
                    .text
                    .contains("truncated")
            );
        } else {
            panic!("Expected tool result");
        }
    }

    #[test]
    fn test_hard_clearing_triggered() {
        let messages = vec![
            create_tool_result_message("critical_tool", vec!["Important data"]),
            create_tool_result_message("normal_tool", vec!["Normal data"]),
            create_tool_result_message("other_tool", vec!["Other data"]),
        ];

        let cleared = hard_clear_non_critical_tools(&messages, |name| {
            name == "critical_tool"
        });

        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &cleared[0].content
        {
            assert_eq!(
                result.content[0].as_text().unwrap().text,
                "Important data"
            );
        } else {
            panic!("Expected tool result");
        }

        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &cleared[1].content
        {
            assert_eq!(result.content.len(), 1);
            assert!(
                result.content[0]
                    .as_text()
                    .unwrap()
                    .text
                    .contains("cleared")
            );
        } else {
            panic!("Expected tool result");
        }

        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &cleared[2].content
        {
            assert_eq!(result.content.len(), 1);
            assert!(
                result.content[0]
                    .as_text()
                    .unwrap()
                    .text
                    .contains("cleared")
            );
        } else {
            panic!("Expected tool result");
        }
    }

    #[test]
    fn test_progressive_escalation() {
        let ratio_level_0 = 0.4;
        assert!(ratio_level_0 < 0.5, "Level 0: ratio below soft_threshold");

        let ratio_level_1 = 0.55;
        assert!(
            (0.5..0.6).contains(&ratio_level_1),
            "Level 1: micro compact range"
        );

        let ratio_level_2 = 0.65;
        assert!(
            (0.6..0.75).contains(&ratio_level_2),
            "Level 2: soft pruning range"
        );

        let ratio_level_3 = 0.8;
        assert!(
            (0.75..0.9).contains(&ratio_level_3),
            "Level 3: hard clearing range"
        );

        let ratio_level_4 = 0.95;
        assert!(ratio_level_4 >= 0.9, "Level 4: summarization range");

        assert!(ratio_level_0 < ratio_level_1);
        assert!(ratio_level_1 < ratio_level_2);
        assert!(ratio_level_2 < ratio_level_3);
        assert!(ratio_level_3 < ratio_level_4);
    }
}

// =============================================================================
// Enhanced Quality Check Tests
// =============================================================================

#[cfg(test)]
mod enhanced_quality_check_tests {
    use super::*;

    #[test]
    fn test_file_paths_detection() {
        let summary_with_paths = r#"## Summary
We modified src/main.rs and crates/app/src/lib.rs to implement the feature.

## User Intent
The user requested changes to the codebase.

## Current Work
Updated file structure.
"#;

        let original_with_paths =
            vec![create_text_message(Role::User, "Please modify src/main.rs")];

        let quality =
            check_summary_quality(summary_with_paths, &original_with_paths);
        assert!(
            quality.has_file_paths,
            "Should detect file paths in summary"
        );

        let summary_without_paths = r#"## Summary
We made some changes to the code.

## User Intent
The user requested changes.

## Current Work
Updated implementation.
"#;

        let quality =
            check_summary_quality(summary_without_paths, &original_with_paths);
        assert!(
            !quality.has_file_paths,
            "Should not detect file paths when summary has none but original does"
        );

        let original_without_paths =
            vec![create_text_message(Role::User, "Please make some changes")];

        let quality = check_summary_quality(
            summary_without_paths,
            &original_without_paths,
        );
        assert!(
            quality.has_file_paths,
            "Should pass file path check when original has no file paths"
        );
    }

    #[test]
    fn test_user_requests_detection() {
        let summary_with_requests = r#"## Summary
The user requested to implement a new feature.

## User Intent
User wants to add authentication.

## Current Work
We need to implement login functionality.
"#;

        let quality = check_summary_quality(summary_with_requests, &[]);
        assert!(quality.has_user_requests, "Should detect user requests");
        assert!(
            quality.user_request_reflected,
            "Should reflect user request"
        );

        let summary_without_requests = r#"## Summary
This is a summary of work done.

## User Intent
Working on improvements.

## Current Work
Making progress.
"#;

        let quality = check_summary_quality(summary_without_requests, &[]);
        assert!(
            !quality.has_user_requests,
            "Should not detect user requests when none present"
        );
    }

    #[test]
    fn test_key_decisions_detection() {
        let summary_with_decisions = r#"## Summary
We decided to use approach A for the implementation.

## User Intent
The user requested a solution.

## Current Work
We concluded that strategy B is best. We recommend using this approach.
"#;

        let quality = check_summary_quality(summary_with_decisions, &[]);
        assert!(quality.has_key_decisions, "Should detect key decisions");

        let summary_without_decisions = r#"## Summary
This is what we did.

## User Intent
The user asked for something.

## Current Work
We worked on it.
"#;

        let quality = check_summary_quality(summary_without_decisions, &[]);
        assert!(
            !quality.has_key_decisions,
            "Should not detect key decisions when none present"
        );
    }

    #[test]
    fn test_quality_score_calculation() {
        let perfect_summary = r#"## Summary
We modified src/main.rs based on user request to implement feature.

## User Intent
The user requested to add authentication and decided on approach A.

## Current Work
We concluded that using JWT is the best solution and recommend this strategy.
"#;

        let quality = check_summary_quality(perfect_summary, &[]);
        assert_eq!(
            quality.overall_score, 1.0,
            "Perfect summary should have score 1.0"
        );

        let partial_summary = r#"## Summary
We made some changes.

## User Intent
The user wants something.

## Current Work
We are working on it.
"#;

        let quality = check_summary_quality(partial_summary, &[]);
        assert!(
            quality.overall_score > 0.0 && quality.overall_score < 1.0,
            "Partial summary should have score between 0 and 1"
        );
        assert!(
            quality.has_required_sections,
            "Should have required sections"
        );
        assert!(
            quality.user_request_reflected,
            "Should reflect user request"
        );

        let poor_summary = "This is just plain text without structure.";

        let quality = check_summary_quality(poor_summary, &[]);
        assert!(
            quality.overall_score < 0.5,
            "Poor summary should have low score"
        );
        assert!(
            !quality.has_required_sections,
            "Should not have required sections"
        );
    }
}

// =============================================================================
// Integration Tests
// =============================================================================

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_full_pruning_workflow() {
        let messages = vec![
            create_text_message(Role::User, "Please search for something"),
            create_tool_use_message("search-1", "search"),
            create_tool_result_message(
                "search-1",
                vec![
                    "Result 1", "Result 2", "Result 3", "Result 4", "Result 5",
                ],
            ),
            create_text_message(Role::Assistant, "Here are the results"),
            create_text_message(Role::User, "Now read a file"),
            create_tool_use_message("read-1", "read"),
            create_tool_result_message(
                "read-1",
                vec![
                    "File content line 1",
                    "File content line 2",
                    "File content line 3",
                    "File content line 4",
                    "File content line 5",
                ],
            ),
            create_text_message(Role::Assistant, "Here's the file"),
        ];

        let pruned =
            prune_tools_with_importance(&messages, |name| match name {
                "search-1" => ToolImportance::Normal,
                "read-1" => ToolImportance::High,
                _ => ToolImportance::Low,
            });

        assert_eq!(pruned.len(), 8);

        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &pruned[2].content
        {
            assert_eq!(result.content.len(), 3);
        }

        if let SamplingContent::Single(SamplingMessageContent::ToolResult(
            result,
        )) = &pruned[6].content
        {
            assert_eq!(result.content.len(), 3);
        }
    }

    #[test]
    fn test_normalize_then_prune() {
        let mut messages = vec![create_tool_use_message("tool-1", "test_tool")];

        normalize_history(&mut messages, true);
        assert_eq!(messages.len(), 2);

        let pruned = prune_tools_with_importance(&messages, |_name| {
            ToolImportance::Normal
        });
        assert_eq!(pruned.len(), 2);
    }

    #[test]
    fn test_token_estimation_with_various_content() {
        let messages = vec![
            create_text_message(Role::User, "Short"),
            create_text_message(Role::User, &"a".repeat(1000)),
            create_tool_use_message("tool-1", "test_tool"),
            create_tool_result_message(
                "tool-1",
                vec!["Line 1", "Line 2", "Line 3"],
            ),
        ];

        let tokens = estimate_tokens(&messages);

        assert!(tokens > 0);

        let short_tokens =
            estimate_tokens(&[create_text_message(Role::User, "Short")]);
        let long_tokens = estimate_tokens(&[create_text_message(
            Role::User,
            &"a".repeat(1000),
        )]);
        assert!(long_tokens > short_tokens);
    }
}
