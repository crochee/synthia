//! Basic processors: deduplicate, merge text, trim whitespace, remove empty.

use std::collections::HashSet;

use synthia_provider::{Content, Message, Role};

use super::super::{
    content_ops::{is_empty_content, merge_text_in_message, trim_text_content},
    keys::compute_message_key,
    pipeline::MessageProcessor,
};

// ---------- 1. deduplicate_messages ----------

/// Drop exact-duplicate messages.
///
/// "Exact duplicate" = same [`Message::role`] and
/// same content signature per [`compute_message_key`].
/// Two textually identical messages from different roles
/// are kept (their `role_prefix` differs).
pub(crate) fn deduplicate_messages(
    messages: Vec<Message>,
) -> (Vec<Message>, Vec<String>) {
    let mut seen = HashSet::new();
    let mut issues = Vec::new();

    let deduped: Vec<Message> = messages
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

// ---------- 2. merge_text_content_items ----------

/// Coalesce adjacent `Text` parts inside a single
/// Assistant message (delegates to
/// [`merge_text_in_message`]). Non-Assistant messages
/// pass through unchanged.
pub(crate) fn merge_text_content_items(
    messages: Vec<Message>,
) -> (Vec<Message>, Vec<String>) {
    messages.into_iter().fold(
        (Vec::new(), Vec::new()),
        |(mut result, mut issues), message| {
            if message.role != Role::Assistant {
                result.push(message);
                return (result, issues);
            }

            let content_len = match &message.content {
                Content::Multi(contents) => contents.len(),
                _ => {
                    result.push(message);
                    return (result, issues);
                }
            };

            let merged = merge_text_in_message(message);
            if let Content::Multi(contents) = &merged.content
                && contents.len() != content_len
            {
                issues.push("Merged text content".to_string());
            }

            result.push(merged);
            (result, issues)
        },
    )
}

// ---------- 3. trim_assistant_text_whitespace ----------

/// Right-trim trailing whitespace from every Assistant
/// message. Other roles pass through unchanged.
pub(crate) fn trim_assistant_text_whitespace(
    messages: Vec<Message>,
) -> (Vec<Message>, Vec<String>) {
    let mut issues = Vec::new();

    let fixed: Vec<Message> = messages
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

// ---------- 4. remove_empty_messages ----------

/// Drop messages whose entire content is empty text
/// (see [`is_empty_content`]).
pub(crate) fn remove_empty_messages(
    messages: Vec<Message>,
) -> (Vec<Message>, Vec<String>) {
    let mut issues = Vec::new();

    let filtered: Vec<Message> = messages
        .into_iter()
        .filter(|msg| {
            let is_empty = match &msg.content {
                Content::Single(content) => is_empty_content(content),
                Content::Multi(contents) => {
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

// Force the imports above to be used when feature flags
// clip them out — they are referenced by the test
// module via `super::`.
#[allow(dead_code)]
pub(super) fn _force_use() {
    let _: MessageProcessor = deduplicate_messages;
}
