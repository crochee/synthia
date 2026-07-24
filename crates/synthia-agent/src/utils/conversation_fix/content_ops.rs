//! Content-level mutation helpers shared by the
//! [`super::processors`] pipeline.
//!
//! These functions are private to the `conversation_fix`
//! module tree. They are NOT processors themselves (a
//! processor operates on `Vec<Message>` and emits an
//! `issues` list); they are the primitives the processors
//! are built from.

use synthia_provider::{Content, ContentPart, Message, Role};

/// Returns `true` when the [`ContentPart`] is an empty
/// `Text`. All other variants are considered non-empty
/// (a tool call, image, audio, etc., is meaningful even
/// without a textual payload).
pub(super) fn is_empty_content(content: &ContentPart) -> bool {
    match content {
        ContentPart::Text(text) => text.text.is_empty(),
        _ => false,
    }
}

/// Right-trim trailing whitespace from every `Text` part
/// in `msg`. Returns `true` when at least one part was
/// modified.
pub(super) fn trim_text_content(msg: &mut Message) -> bool {
    let mut modified = false;

    match &mut msg.content {
        Content::Single(ContentPart::Text(text)) => {
            let trimmed = text.text.trim_end();
            if trimmed.len() != text.text.len() {
                text.text = trimmed.to_string();
                modified = true;
            }
        }
        Content::Single(ContentPart::ToolResult(_)) => {}
        Content::Single(ContentPart::ToolUse(_)) => {}
        Content::Single(ContentPart::Image(_)) => {}
        Content::Single(ContentPart::Audio(_)) => {}
        Content::Single(ContentPart::Reasoning(_)) => {}
        Content::Single(ContentPart::Resource(_)) => {}
        Content::Multi(contents) => {
            for content in contents.iter_mut() {
                if let ContentPart::Text(text) = content {
                    let trimmed = text.text.trim_end();
                    if trimmed.len() != text.text.len() {
                        text.text = trimmed.to_string();
                        modified = true;
                    }
                }
            }
        }
    }

    modified
}

/// Resolve the "effective role" of `msg` for merge
/// purposes.
///
/// Some providers (notably Anthropic) deliver tool
/// results as `Role::User` messages whose content is a
/// `ToolResult`. For the merge logic those should be
/// treated as `tool` messages — otherwise a "user → tool
/// result" pair would be collapsed into a single
/// `user` message and lose the tool result.
pub(super) fn effective_role(msg: &Message) -> String {
    let has_tool_result = match &msg.content {
        Content::Single(content) => {
            matches!(content, ContentPart::ToolResult(_))
        }
        Content::Multi(contents) => contents
            .iter()
            .any(|c| matches!(c, ContentPart::ToolResult(_))),
    };
    if msg.role == Role::User && has_tool_result {
        "tool".to_string()
    } else {
        match msg.role {
            Role::User => "user".to_string(),
            Role::Assistant => "assistant".to_string(),
            Role::System => "system".to_string(),
            Role::Tool => "tool".to_string(),
        }
    }
}

/// Move the contents of `source` into `target`, in order.
/// If `target` is currently `Content::Single`, the result
/// is `Content::Multi`; otherwise `source`'s parts are
/// appended to the existing list.
pub(super) fn merge_messages(target: &mut Message, source: Message) {
    let source_contents = match source.content {
        Content::Single(content) => vec![content],
        Content::Multi(contents) => contents,
    };

    match &mut target.content {
        Content::Single(content) => {
            let mut contents = vec![content.clone()];
            contents.extend(source_contents);
            target.content = Content::Multi(contents);
        }
        Content::Multi(contents) => {
            contents.extend(source_contents);
        }
    }
}

/// Merge adjacent `Text` parts of a single Assistant
/// message. Non-text parts act as breakers — `text | tool_use
/// | text` does NOT collapse across the tool use.
///
/// If after merging only one part remains, the message
/// is downgraded to `Content::Single` for compactness.
pub(super) fn merge_text_in_message(mut msg: Message) -> Message {
    if msg.role != Role::Assistant {
        return msg;
    }

    let contents = match msg.content {
        Content::Multi(c) => c,
        _ => return msg,
    };

    let merged = contents.into_iter().fold(Vec::new(), |mut acc, item| {
        match item {
            ContentPart::Text(text) => {
                if let Some(ContentPart::Text(last)) = acc.last_mut() {
                    last.text.push_str(&text.text);
                } else {
                    acc.push(ContentPart::Text(text));
                }
            }
            other => acc.push(other),
        }
        acc
    });

    let merged_len = merged.len();
    msg.content = if merged_len == 1 {
        let content =
            merged.into_iter().next().unwrap_or_else(|| unreachable!());
        Content::Single(content)
    } else {
        Content::Multi(merged)
    };

    msg
}
