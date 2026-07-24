//! Processor 5: fix_tool_calling — enforce tool-call/tool-result invariants.

use std::collections::HashSet;

use synthia_provider::{Content, ContentPart, Message, Role};

use super::basic::remove_empty_messages;

/// Repair broken tool-calling message shapes.
///
/// This is the most involved processor. It enforces three
/// invariants on the tool-call / tool-result protocol:
///
/// 1. `ToolUse` parts are only allowed inside Assistant
///    messages. Any `ToolUse` found in a User message
///    is removed.
/// 2. `ToolResult` parts are only allowed inside User /
///    Tool messages. Any `ToolResult` found in an
///    Assistant message is removed.
/// 3. Every `ToolResult` must reference a `ToolUse` that
///    is still "pending" (a `ToolUse.id` issued earlier
///    in the conversation). Orphaned results are dropped
///    in a second pass — we cannot decide "orphan" while
///    we are still tracking pending uses, since the use
///    that resolves them may itself be removed.
///
/// The function also calls [`remove_empty_messages`] at
/// the end: pruning tool uses from an Assistant message
/// may leave behind an empty message.
pub(crate) fn fix_tool_calling(
    messages: Vec<Message>,
) -> (Vec<Message>, Vec<String>) {
    let mut issues = Vec::new();
    let mut pending_tool_uses: HashSet<String> = HashSet::new();

    let mut fixed_messages: Vec<Message> = Vec::new();

    for mut message in messages {
        let mut content_to_remove: Vec<usize> = Vec::new();

        match message.role {
            Role::User => {
                if let Content::Multi(ref contents) = message.content {
                    for (idx, content) in contents.iter().enumerate() {
                        match content {
                            ContentPart::ToolUse(tool_use) => {
                                content_to_remove.push(idx);
                                issues.push(format!(
                                    "Removed tool use '{}' from user message",
                                    tool_use.id
                                ));
                            }
                            ContentPart::ToolResult(tool_result) => {
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
                } else if let Content::Single(ref content) = message.content {
                    match content {
                        ContentPart::ToolUse(tool_use) => {
                            issues.push(format!(
                                "Removed tool use '{}' from user message",
                                tool_use.id
                            ));
                            continue;
                        }
                        ContentPart::ToolResult(tool_result) => {
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
                if let Content::Multi(ref contents) = message.content {
                    for (idx, content) in contents.iter().enumerate() {
                        match content {
                            ContentPart::ToolResult(tool_result) => {
                                content_to_remove.push(idx);
                                issues.push(format!(
                                    "Removed tool result '{}' from assistant message",
                                    tool_result.tool_use_id
                                ));
                            }
                            ContentPart::ToolUse(tool_use) => {
                                pending_tool_uses.insert(tool_use.id.clone());
                            }
                            _ => {}
                        }
                    }
                } else if let Content::Single(ref content) = message.content {
                    match content {
                        ContentPart::ToolResult(tool_result) => {
                            issues.push(format!(
                                "Removed tool result '{}' from assistant message",
                                tool_result.tool_use_id
                            ));
                            continue;
                        }
                        ContentPart::ToolUse(tool_use) => {
                            pending_tool_uses.insert(tool_use.id.clone());
                        }
                        _ => {}
                    }
                }
            }
            Role::Tool => {
                if let Content::Single(ContentPart::ToolResult(tr)) =
                    &message.content
                    && pending_tool_uses.contains(&tr.tool_use_id)
                {
                    pending_tool_uses.remove(&tr.tool_use_id);
                }
            }
            _ => {}
        }

        if let Content::Multi(ref mut contents) = message.content {
            for &idx in content_to_remove.iter().rev() {
                contents.remove(idx);
            }
        }

        fixed_messages.push(message);
    }

    // Second loop - check for orphaned tool uses
    let mut final_messages: Vec<Message> = Vec::new();
    for mut message in fixed_messages {
        if message.role == Role::Assistant {
            let mut content_to_remove: Vec<usize> = Vec::new();

            if let Content::Multi(ref contents) = message.content {
                for (idx, content) in contents.iter().enumerate() {
                    if let ContentPart::ToolUse(tool_use) = content
                        && pending_tool_uses.contains(&tool_use.id)
                    {
                        content_to_remove.push(idx);
                    }
                }
            } else if let Content::Single(ref content) = message.content
                && let ContentPart::ToolUse(tool_use) = content
                && pending_tool_uses.contains(&tool_use.id)
            {
                continue;
            }

            if let Content::Multi(ref mut contents) = message.content {
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
