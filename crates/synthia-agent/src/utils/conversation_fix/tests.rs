//! Unit tests for the `conversation_fix` pipeline.
//!
//! Every test feeds a hand-built `Vec<Message>` to
//! [`super::pipeline::fix_conversation`] and asserts on
//! both the fixed messages and the reported issues.
//!
//! The `run_verify` helper pins the **idempotency
//! invariant**: running `fix_conversation` twice on the
//! same input must produce zero new issues on the second
//! pass. This catches processor-ordering bugs where one
//! pass would leave residual state for the next.

#[cfg(test)]
mod tests {
    use synthia_provider::{
        Content,
        ContentPart,
        Message,
        Role,
        TextContent,
        ToolResult,
        ToolUse,
    };

    use super::super::{
        content_ops::{effective_role, trim_text_content},
        keys::compute_message_key,
        pipeline::fix_conversation,
    };

    fn create_text_message(role: Role, text: &str) -> Message {
        Message {
            role,
            content: Content::Single(ContentPart::Text(TextContent {
                text: text.to_string(),
                cache_control: None,
            })),
            tool_call_id: None,
            name: None,
            ..Default::default()
        }
    }

    fn create_tool_use_message(role: Role, id: &str, name: &str) -> Message {
        Message {
            role,
            content: Content::Single(ContentPart::ToolUse(ToolUse {
                id: id.to_string(),
                name: name.to_string(),
                input: serde_json::json!({}),
            })),
            tool_call_id: None,
            name: None,
            ..Default::default()
        }
    }

    fn create_tool_result_message(
        role: Role,
        tool_use_id: &str,
        result_text: &str,
    ) -> Message {
        Message {
            role,
            content: Content::Single(ContentPart::ToolResult(ToolResult::new(
                tool_use_id,
                result_text,
            ))),
            tool_call_id: None,
            name: None,
            ..Default::default()
        }
    }

    fn run_verify(messages: Vec<Message>) -> (Vec<Message>, Vec<String>) {
        let (messages, issues) = fix_conversation(messages);

        let (_, second_issues) = fix_conversation(messages.clone());
        assert!(
            second_issues.is_empty(),
            "Fixed conversation should have no issues, but found: {second_issues:?}"
        );

        (messages, issues)
    }

    #[test]
    fn test_valid_conversation() {
        let messages = vec![
            create_text_message(Role::User, "Hello"),
            create_text_message(Role::Assistant, "Hi there!"),
            create_text_message(Role::User, "How are you?"),
        ];

        let (fixed, issues) = run_verify(messages);
        assert_eq!(fixed.len(), 3);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_merge_consecutive_user_messages() {
        let messages = vec![
            create_text_message(Role::User, "Hello"),
            create_text_message(Role::User, "World"),
            create_text_message(Role::Assistant, "Hi!"),
            create_text_message(Role::User, "Thanks"),
        ];

        let (fixed, issues) = run_verify(messages);
        assert_eq!(fixed.len(), 3);
        assert!(issues.iter().any(|i| i.contains("Merged consecutive")));
    }

    #[test]
    fn test_remove_leading_assistant() {
        let messages = vec![
            create_text_message(Role::Assistant, "I should not be here"),
            create_text_message(Role::User, "Hello"),
            create_text_message(Role::Assistant, "Hi!"),
        ];

        let (fixed, issues) = run_verify(messages);
        assert_eq!(fixed.len(), 1);
        assert!(issues.iter().any(|i| i.contains("Removed leading")));
        assert!(issues.iter().any(|i| i.contains("Removed trailing")));
    }

    #[test]
    fn test_empty_conversation() {
        let messages = vec![];

        let (fixed, issues) = run_verify(messages);
        assert_eq!(fixed.len(), 1);
        assert_eq!(fixed[0].role, Role::User);
        assert!(issues.iter().any(|i| i.contains("placeholder")));
    }

    #[test]
    fn test_deduplicate_messages() {
        let messages = vec![
            create_text_message(Role::User, "Hello"),
            create_text_message(Role::User, "Hello"),
            create_text_message(Role::Assistant, "Hi!"),
            create_text_message(Role::User, "Thanks"),
        ];

        let (fixed, issues) = run_verify(messages);
        assert_eq!(fixed.len(), 3);
        assert!(issues.iter().any(|i| i.contains("duplicate")));
    }

    #[test]
    fn test_fix_orphaned_tool_result() {
        let messages = vec![
            create_text_message(Role::User, "Hello"),
            create_tool_result_message(Role::User, "orphan-id", "result"),
            create_text_message(Role::Assistant, "Hi!"),
        ];

        let (_fixed, issues) = run_verify(messages);
        assert!(issues.iter().any(|i| i.contains("orphaned tool result")));
    }

    #[test]
    fn test_tool_use_result_pairing() {
        let messages = vec![
            create_text_message(Role::User, "Search for something"),
            create_tool_use_message(Role::Assistant, "search-1", "web_search"),
            create_tool_result_message(Role::User, "search-1", "results"),
            create_text_message(Role::Assistant, "Here are the results"),
            create_text_message(Role::User, "Thanks"),
        ];

        let (fixed, issues) = run_verify(messages);
        assert_eq!(fixed.len(), 5);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_trim_whitespace() {
        let mut msg = create_text_message(Role::Assistant, "Hello   ");
        let modified = trim_text_content(&mut msg);
        assert!(modified);

        match &msg.content {
            Content::Single(ContentPart::Text(text)) => {
                assert_eq!(text.text, "Hello");
            }
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_remove_empty_messages() {
        let messages = vec![
            create_text_message(Role::User, ""),
            create_text_message(Role::User, "Hello"),
            create_text_message(Role::Assistant, ""),
        ];

        let (fixed, issues) = run_verify(messages);
        assert_eq!(fixed.len(), 1);
        assert!(issues.iter().any(|i| i.contains("Removed empty")));
    }

    #[test]
    fn test_complex_tool_chain() {
        let messages = vec![
            create_text_message(Role::User, "Do task A and B"),
            create_tool_use_message(Role::Assistant, "tool-1", "task_a"),
            create_tool_result_message(Role::User, "tool-1", "result a"),
            create_tool_use_message(Role::Assistant, "tool-2", "task_b"),
            create_tool_result_message(Role::User, "tool-2", "result b"),
            create_text_message(Role::Assistant, "Done!"),
            create_text_message(Role::User, "Thanks"),
        ];

        let (fixed, _issues) = run_verify(messages);
        assert_eq!(fixed.len(), 7, "Tool pairs should not be merged");
    }

    #[test]
    fn test_compute_message_key() {
        let msg1 = create_text_message(Role::User, "Hello");
        let msg2 = create_text_message(Role::User, "Hello");
        let msg3 = create_text_message(Role::User, "World");

        assert_eq!(compute_message_key(&msg1), compute_message_key(&msg2));
        assert_ne!(compute_message_key(&msg1), compute_message_key(&msg3));
    }

    #[test]
    fn test_effective_role() {
        let text_msg = create_text_message(Role::User, "Hello");
        assert_eq!(effective_role(&text_msg), "user");

        let tool_result_msg =
            create_tool_result_message(Role::User, "tool-1", "result");
        assert_eq!(effective_role(&tool_result_msg), "tool");
    }

    #[test]
    fn test_tool_pairs_not_merged() {
        let messages = vec![
            create_text_message(Role::User, "Do task A and B"),
            create_tool_use_message(Role::Assistant, "tool-1", "task_a"),
            create_tool_result_message(Role::User, "tool-1", "result a"),
            create_tool_use_message(Role::Assistant, "tool-2", "task_b"),
            create_tool_result_message(Role::User, "tool-2", "result b"),
            create_text_message(Role::Assistant, "Done!"),
            create_text_message(Role::User, "Thanks"),
        ];

        let (fixed, _issues) = fix_conversation(messages);

        assert_eq!(fixed.len(), 7, "Tool pairs should not be merged");

        assert!(is_tool_use(&fixed[1], "tool-1"));
        assert!(is_tool_result(&fixed[2], "tool-1"));
        assert!(is_tool_use(&fixed[3], "tool-2"));
        assert!(is_tool_result(&fixed[4], "tool-2"));
    }

    #[test]
    fn test_consecutive_tool_results_not_merged() {
        let messages = vec![
            create_text_message(Role::User, "Start"),
            create_tool_use_message(Role::Assistant, "tool-1", "task_a"),
            create_tool_result_message(Role::User, "tool-1", "result a"),
            create_tool_use_message(Role::Assistant, "tool-2", "task_b"),
            create_tool_result_message(Role::User, "tool-2", "result b"),
            create_tool_use_message(Role::Assistant, "tool-3", "task_c"),
            create_tool_result_message(Role::User, "tool-3", "result c"),
            create_text_message(Role::User, "End"),
        ];

        let (fixed, _issues) = fix_conversation(messages);

        assert_eq!(
            fixed.len(),
            8,
            "Consecutive tool results should not be merged"
        );

        for i in (0..6).step_by(2) {
            assert!(
                is_tool_use(&fixed[i + 1], &format!("tool-{}", i / 2 + 1)),
                "Message {} should be ToolUse",
                i + 1
            );
            assert!(
                is_tool_result(&fixed[i + 2], &format!("tool-{}", i / 2 + 1)),
                "Message {} should be ToolResult for tool-{}",
                i + 2,
                i / 2 + 1
            );
        }
    }

    #[test]
    fn test_tool_result_not_merged_with_user_text() {
        let messages = vec![
            create_text_message(Role::User, "Start"),
            create_tool_use_message(Role::Assistant, "tool-1", "task"),
            create_tool_result_message(Role::User, "tool-1", "result"),
            create_text_message(Role::User, "Additional context"),
        ];

        let (fixed, _issues) = fix_conversation(messages);

        assert_eq!(fixed.len(), 4);

        assert!(is_tool_use(&fixed[1], "tool-1"));
        assert!(is_tool_result(&fixed[2], "tool-1"));
        match &fixed[3].content {
            Content::Single(ContentPart::Text(text)) => {
                assert_eq!(text.text, "Additional context");
            }
            _ => panic!("Message 3 should be text content"),
        }
    }

    fn is_tool_use(msg: &Message, expected_id: &str) -> bool {
        matches!(
            &msg.content,
            Content::Single(ContentPart::ToolUse(tool_use))
                if tool_use.id == expected_id
        )
    }

    fn is_tool_result(msg: &Message, expected_id: &str) -> bool {
        matches!(
            &msg.content,
            Content::Single(ContentPart::ToolResult(result))
                if result.tool_use_id == expected_id
        )
    }
}
