use std::collections::HashSet;

use rmcp::model::{
    Role,
    SamplingContent,
    SamplingMessage,
    SamplingMessageContent,
};

fn content_to_key(content: &SamplingMessageContent) -> String {
    match content {
        SamplingMessageContent::Text(text) => format!("T:{}", text.text),
        SamplingMessageContent::ToolUse(tool_use) => {
            format!("TU:{}:{}", tool_use.id, tool_use.name)
        }
        SamplingMessageContent::ToolResult(tool_result) => {
            format!("TR:{}", tool_result.tool_use_id)
        }
        SamplingMessageContent::Image(_) => "IMG".to_string(),
        SamplingMessageContent::Audio(_) => "AUD".to_string(),
    }
}

fn compute_message_key(msg: &SamplingMessage) -> String {
    let role_prefix = match msg.role {
        Role::User => "U:",
        Role::Assistant => "A:",
    };

    let content_str = match &msg.content {
        SamplingContent::Single(content) => content_to_key(content),
        SamplingContent::Multiple(contents) => contents
            .iter()
            .map(content_to_key)
            .collect::<Vec<_>>()
            .join("|"),
    };

    format!("{role_prefix}{content_str}")
}

fn is_empty_content(content: &SamplingMessageContent) -> bool {
    match content {
        SamplingMessageContent::Text(text) => text.text.is_empty(),
        _ => false,
    }
}

fn trim_text_content(msg: &mut SamplingMessage) -> bool {
    let mut modified = false;

    match &mut msg.content {
        SamplingContent::Single(SamplingMessageContent::Text(text)) => {
            let trimmed = text.text.trim_end();
            if trimmed.len() != text.text.len() {
                text.text = trimmed.to_string();
                modified = true;
            }
        }
        SamplingContent::Multiple(contents) => {
            for content in contents.iter_mut() {
                if let SamplingMessageContent::Text(text) = content {
                    let trimmed = text.text.trim_end();
                    if trimmed.len() != text.text.len() {
                        text.text = trimmed.to_string();
                        modified = true;
                    }
                }
            }
        }
        _ => {}
    }

    modified
}

fn remove_empty_messages(
    messages: Vec<SamplingMessage>,
) -> (Vec<SamplingMessage>, Vec<String>) {
    let mut issues = Vec::new();

    let filtered: Vec<SamplingMessage> = messages
        .into_iter()
        .filter(|msg| {
            let is_empty = match &msg.content {
                SamplingContent::Single(content) => is_empty_content(content),
                SamplingContent::Multiple(contents) => {
                    contents.iter().all(is_empty_content)
                }
            };

            if is_empty {
                issues.push("Removed empty message".to_string());
                false
            } else {
                true
            }
        })
        .collect();

    (filtered, issues)
}

fn effective_role(msg: &SamplingMessage) -> String {
    let has_tool_result = match &msg.content {
        SamplingContent::Single(content) => {
            matches!(content, SamplingMessageContent::ToolResult(_))
        }
        SamplingContent::Multiple(contents) => contents
            .iter()
            .any(|c| matches!(c, SamplingMessageContent::ToolResult(_))),
    };
    if msg.role == Role::User && has_tool_result {
        "tool".to_string()
    } else {
        match msg.role {
            Role::User => "user".to_string(),
            Role::Assistant => "assistant".to_string(),
        }
    }
}

fn merge_messages(target: &mut SamplingMessage, source: SamplingMessage) {
    let source_contents = match source.content {
        SamplingContent::Single(content) => vec![content],
        SamplingContent::Multiple(contents) => contents,
    };

    match &mut target.content {
        SamplingContent::Single(content) => {
            let mut contents = vec![content.clone()];
            contents.extend(source_contents);
            target.content = SamplingContent::Multiple(contents);
        }
        SamplingContent::Multiple(contents) => {
            contents.extend(source_contents);
        }
    }
}

pub(crate) fn deduplicate_messages(
    messages: Vec<SamplingMessage>,
) -> (Vec<SamplingMessage>, Vec<String>) {
    let mut seen = HashSet::new();
    let mut issues = Vec::new();

    let deduped: Vec<SamplingMessage> = messages
        .into_iter()
        .filter(|msg| {
            let key = compute_message_key(msg);
            if seen.contains(&key) {
                issues.push("Removed duplicate message".to_string());
                false
            } else {
                seen.insert(key);
                true
            }
        })
        .collect();

    (deduped, issues)
}

pub(crate) fn merge_text_content_items(
    messages: Vec<SamplingMessage>,
) -> (Vec<SamplingMessage>, Vec<String>) {
    messages.into_iter().fold(
        (Vec::new(), Vec::new()),
        |(mut result, mut issues), message| {
            if message.role != Role::Assistant {
                result.push(message);
                return (result, issues);
            }

            let content_len = match &message.content {
                SamplingContent::Multiple(contents) => contents.len(),
                _ => {
                    result.push(message);
                    return (result, issues);
                }
            };

            let merged = merge_text_in_message(message);
            if let SamplingContent::Multiple(contents) = &merged.content
                && contents.len() != content_len
            {
                issues.push("Merged text content".to_string());
            }

            result.push(merged);
            (result, issues)
        },
    )
}

fn merge_text_in_message(mut msg: SamplingMessage) -> SamplingMessage {
    if msg.role != Role::Assistant {
        return msg;
    }

    let contents = match msg.content {
        SamplingContent::Multiple(c) => c,
        _ => return msg,
    };

    let merged = contents.into_iter().fold(Vec::new(), |mut acc, item| {
        match item {
            SamplingMessageContent::Text(text) => {
                if let Some(SamplingMessageContent::Text(last)) = acc.last_mut()
                {
                    last.text.push_str(&text.text);
                } else {
                    acc.push(SamplingMessageContent::Text(text));
                }
            }
            other => acc.push(other),
        }
        acc
    });

    let merged_len = merged.len();
    msg.content = if merged_len == 1 {
        let content = merged.into_iter().next().unwrap_or_else(|| {
            panic!("merged has exactly one element but next() returned None")
        });
        SamplingContent::Single(content)
    } else {
        SamplingContent::Multiple(merged)
    };

    msg
}

pub(crate) fn trim_assistant_text_whitespace(
    messages: Vec<SamplingMessage>,
) -> (Vec<SamplingMessage>, Vec<String>) {
    let mut issues = Vec::new();

    let fixed: Vec<SamplingMessage> = messages
        .into_iter()
        .map(|mut message| {
            if message.role == Role::Assistant {
                let modified = trim_text_content(&mut message);
                if modified {
                    issues.push(
                        "Trimmed trailing whitespace from assistant message"
                            .to_string(),
                    );
                }
            }
            message
        })
        .collect();

    (fixed, issues)
}

pub(crate) fn fix_lead_trail(
    messages: Vec<SamplingMessage>,
) -> (Vec<SamplingMessage>, Vec<String>) {
    let mut issues = Vec::new();
    let mut result = messages;

    while let Some(first) = result.first() {
        if first.role == Role::Assistant {
            result.remove(0);
            issues.push("Removed leading assistant message".to_string());
        } else {
            break;
        }
    }

    while let Some(last) = result.last() {
        if last.role == Role::Assistant {
            result.pop();
            issues.push("Removed trailing assistant message".to_string());
        } else {
            break;
        }
    }

    let (result, lead_trail_issues) = remove_empty_messages(result);
    issues.extend(lead_trail_issues);

    (result, issues)
}

pub(crate) fn populate_if_empty(
    messages: Vec<SamplingMessage>,
) -> (Vec<SamplingMessage>, Vec<String>) {
    const PLACEHOLDER_USER_MESSAGE: &str = "Hello";

    if messages.is_empty() {
        let placeholder = SamplingMessage {
            role: Role::User,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                rmcp::model::RawTextContent {
                    text: PLACEHOLDER_USER_MESSAGE.to_string(),
                    meta: None,
                },
            )),
            meta: None,
        };
        (
            vec![placeholder],
            vec![
                "Added placeholder user message to empty conversation"
                    .to_string(),
            ],
        )
    } else {
        (messages, Vec::new())
    }
}

pub(crate) fn merge_consecutive_messages(
    messages: Vec<SamplingMessage>,
) -> (Vec<SamplingMessage>, Vec<String>) {
    let mut issues = Vec::new();
    let mut merged_messages: Vec<SamplingMessage> = Vec::new();

    for message in messages {
        if let Some(last) = merged_messages.last_mut() {
            let last_effective = effective_role(last);
            let current_effective = effective_role(&message);

            if last_effective == current_effective && last_effective != "tool" {
                merge_messages(last, message);
                issues.push(format!(
                    "Merged consecutive {current_effective} messages"
                ));
                continue;
            }
        }
        merged_messages.push(message);
    }

    (merged_messages, issues)
}

pub(crate) fn fix_tool_calling(
    messages: Vec<SamplingMessage>,
) -> (Vec<SamplingMessage>, Vec<String>) {
    let mut issues = Vec::new();
    let mut pending_tool_uses: HashSet<String> = HashSet::new();

    let mut fixed_messages: Vec<SamplingMessage> = Vec::new();

    for mut message in messages {
        let mut content_to_remove: Vec<usize> = Vec::new();

        match message.role {
            Role::User => {
                if let SamplingContent::Multiple(ref contents) = message.content
                {
                    for (idx, content) in contents.iter().enumerate() {
                        match content {
                            SamplingMessageContent::ToolUse(tool_use) => {
                                content_to_remove.push(idx);
                                issues.push(format!(
                                    "Removed tool use '{}' from user message",
                                    tool_use.id
                                ));
                            }
                            SamplingMessageContent::ToolResult(tool_result) => {
                                if pending_tool_uses
                                    .contains(&tool_result.tool_use_id)
                                {
                                    pending_tool_uses
                                        .remove(&tool_result.tool_use_id);
                                } else {
                                    content_to_remove.push(idx);
                                    issues.push(format!(
                                        "Removed orphaned tool result '{}'",
                                        tool_result.tool_use_id
                                    ));
                                }
                            }
                            _ => {}
                        }
                    }
                } else if let SamplingContent::Single(ref content) =
                    message.content
                {
                    match content {
                        SamplingMessageContent::ToolUse(tool_use) => {
                            issues.push(format!(
                                "Removed tool use '{}' from user message",
                                tool_use.id
                            ));
                            continue;
                        }
                        SamplingMessageContent::ToolResult(tool_result) => {
                            if pending_tool_uses
                                .contains(&tool_result.tool_use_id)
                            {
                                pending_tool_uses
                                    .remove(&tool_result.tool_use_id);
                            } else {
                                issues.push(format!(
                                    "Removed orphaned tool result '{}'",
                                    tool_result.tool_use_id
                                ));
                                continue;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Role::Assistant => {
                if let SamplingContent::Multiple(ref contents) = message.content
                {
                    for (idx, content) in contents.iter().enumerate() {
                        match content {
                            SamplingMessageContent::ToolResult(tool_result) => {
                                content_to_remove.push(idx);
                                issues.push(format!(
                                    "Removed tool result '{}' from assistant message",
                                    tool_result.tool_use_id
                                ));
                            }
                            SamplingMessageContent::ToolUse(tool_use) => {
                                pending_tool_uses.insert(tool_use.id.clone());
                            }
                            _ => {}
                        }
                    }
                } else if let SamplingContent::Single(ref content) =
                    message.content
                {
                    match content {
                        SamplingMessageContent::ToolResult(tool_result) => {
                            issues.push(format!(
                                "Removed tool result '{}' from assistant message",
                                tool_result.tool_use_id
                            ));
                            continue;
                        }
                        SamplingMessageContent::ToolUse(tool_use) => {
                            pending_tool_uses.insert(tool_use.id.clone());
                        }
                        _ => {}
                    }
                }
            }
        }

        if let SamplingContent::Multiple(ref mut contents) = message.content {
            for &idx in content_to_remove.iter().rev() {
                contents.remove(idx);
            }
        }

        fixed_messages.push(message);
    }

    let mut final_messages: Vec<SamplingMessage> = Vec::new();
    for mut message in fixed_messages {
        if message.role == Role::Assistant {
            let mut content_to_remove: Vec<usize> = Vec::new();

            if let SamplingContent::Multiple(ref contents) = message.content {
                for (idx, content) in contents.iter().enumerate() {
                    if let SamplingMessageContent::ToolUse(tool_use) = content
                        && pending_tool_uses.contains(&tool_use.id)
                    {
                        content_to_remove.push(idx);
                        issues.push(format!(
                            "Removed orphaned tool use '{}'",
                            tool_use.id
                        ));
                    }
                }
            } else if let SamplingContent::Single(ref content) = message.content
                && let SamplingMessageContent::ToolUse(tool_use) = content
                && pending_tool_uses.contains(&tool_use.id)
            {
                issues.push(format!(
                    "Removed orphaned tool use '{}'",
                    tool_use.id
                ));
                continue;
            }

            if let SamplingContent::Multiple(ref mut contents) = message.content
            {
                for &idx in content_to_remove.iter().rev() {
                    contents.remove(idx);
                }
            }
        }

        final_messages.push(message);
    }

    let (final_messages, empty_issues) = remove_empty_messages(final_messages);
    issues.extend(empty_issues);

    (final_messages, issues)
}

type MessageProcessor =
    fn(Vec<SamplingMessage>) -> (Vec<SamplingMessage>, Vec<String>);

pub fn fix_conversation(
    messages: Vec<SamplingMessage>,
) -> (Vec<SamplingMessage>, Vec<String>) {
    let processors: Vec<MessageProcessor> = vec![
        deduplicate_messages,
        merge_text_content_items,
        trim_assistant_text_whitespace,
        remove_empty_messages,
        fix_tool_calling,
        merge_consecutive_messages,
        fix_lead_trail,
        populate_if_empty,
    ];

    processors.into_iter().fold(
        (messages, Vec::new()),
        |(msgs, issues), processor| {
            let (new_msgs, new_issues) = processor(msgs);
            (new_msgs, issues.into_iter().chain(new_issues).collect())
        },
    )
}

#[cfg(test)]
mod tests {
    use rmcp::model::{
        Content,
        Role,
        SamplingContent,
        SamplingMessage,
        SamplingMessageContent,
        ToolResultContent,
        ToolUseContent,
    };

    use super::*;

    fn create_text_message(role: Role, text: &str) -> SamplingMessage {
        SamplingMessage {
            role,
            content: SamplingContent::Single(SamplingMessageContent::Text(
                rmcp::model::RawTextContent {
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
    ) -> SamplingMessage {
        SamplingMessage {
            role,
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
        role: Role,
        tool_use_id: &str,
        result_text: &str,
    ) -> SamplingMessage {
        SamplingMessage {
            role,
            content: SamplingContent::Single(
                SamplingMessageContent::ToolResult(ToolResultContent::new(
                    tool_use_id,
                    vec![Content::text(result_text)],
                )),
            ),
            meta: None,
        }
    }

    fn run_verify(
        messages: Vec<SamplingMessage>,
    ) -> (Vec<SamplingMessage>, Vec<String>) {
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
            SamplingContent::Single(SamplingMessageContent::Text(text)) => {
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
            SamplingContent::Single(SamplingMessageContent::Text(text)) => {
                assert_eq!(text.text, "Additional context");
            }
            _ => panic!("Message 3 should be text content"),
        }
    }

    fn is_tool_use(msg: &SamplingMessage, expected_id: &str) -> bool {
        matches!(
            &msg.content,
            SamplingContent::Single(SamplingMessageContent::ToolUse(tool_use))
                if tool_use.id == expected_id
        )
    }

    fn is_tool_result(msg: &SamplingMessage, expected_id: &str) -> bool {
        matches!(
            &msg.content,
            SamplingContent::Single(SamplingMessageContent::ToolResult(result))
                if result.tool_use_id == expected_id
        )
    }
}
