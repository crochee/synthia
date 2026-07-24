//! Aggregate processors: merge consecutive, fix lead/trail, populate if empty.

use synthia_provider::{Message, Role};

use super::{
    super::content_ops::{effective_role, merge_messages},
    basic::remove_empty_messages,
};

// ---------- 6. merge_consecutive_messages ----------

/// Merge consecutive messages of the same effective role
/// (see [`effective_role`]). Tool messages are NEVER
/// merged — the merge logic must preserve every
/// `ToolUse` / `ToolResult` boundary verbatim, otherwise
/// the tool-call protocol breaks.
pub(crate) fn merge_consecutive_messages(
    messages: Vec<Message>,
) -> (Vec<Message>, Vec<String>) {
    let mut issues = Vec::new();
    let mut merged_messages: Vec<Message> = Vec::new();

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

// ---------- 7. fix_lead_trail ----------

/// Strip leading and trailing Assistant messages and drop
/// any leftover empty messages.
///
/// Assistants at the conversation boundary are a sign
/// that the previous `merge_consecutive_messages` pass
/// was not enough (e.g., a checkpoint resumed from a
/// half-finished tool call).
pub(crate) fn fix_lead_trail(
    messages: Vec<Message>,
) -> (Vec<Message>, Vec<String>) {
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

// ---------- 8. populate_if_empty ----------

/// Insert a placeholder "Hello" user message when the
/// conversation is empty, so downstream code never has
/// to handle a 0-length `Vec<Message>`.
pub(crate) fn populate_if_empty(
    messages: Vec<Message>,
) -> (Vec<Message>, Vec<String>) {
    const PLACEHOLDER_USER_MESSAGE: &str = "Hello";

    if messages.is_empty() {
        let placeholder = Message::user(PLACEHOLDER_USER_MESSAGE);
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
