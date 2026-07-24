//! Key-building helpers for [`Message`] deduplication.
//!
//! [`Message`]: synthia_provider::types::Message

use synthia_provider::{Content, ContentPart, Message, Role};

/// Build a stable string key for a single [`ContentPart`].
///
/// Two [`ContentPart`]s that should be treated as the "same"
/// for dedup purposes produce the same key:
///
/// - `Text` is keyed by its raw text (no length cap)
/// - `ToolUse` is keyed by `(id, name)`
/// - `ToolResult` is keyed by `tool_use_id`
/// - `Image` / `Audio` / `Reasoning` / `Resource` collapse to
///   a fixed marker — the fields inside them are intentionally
///   ignored for the dedup key (they may legitimately carry
///   varying binary / opaque payloads that should NOT
///   differentiate messages).
fn content_to_key(content: &ContentPart) -> String {
    match content {
        ContentPart::Text(text) => format!("T:{}", text.text),
        ContentPart::ToolUse(tool_use) => {
            format!("TU:{}:{}", tool_use.id, tool_use.name)
        }
        ContentPart::ToolResult(tool_result) => {
            format!("TR:{}", tool_result.tool_use_id)
        }
        ContentPart::Image(_) => "IMG".to_string(),
        ContentPart::Audio(_) => "AUD".to_string(),
        ContentPart::Reasoning(_) => "RSN".to_string(),
        ContentPart::Resource(_) => "RES".to_string(),
    }
}

/// Build a stable string key for a [`Message`].
///
/// Key shape: `<role_prefix><content_signature>` where the
/// content signature is the `|`-joined per-part key from
/// [`content_to_key`]. The role prefix differentiates
/// messages whose content shape is identical but whose
/// speaker differs (e.g., a user "OK" and an assistant "OK").
pub(super) fn compute_message_key(msg: &Message) -> String {
    let role_prefix = match msg.role {
        Role::User => "U:",
        Role::Assistant => "A:",
        Role::System => "S:",
        Role::Tool => "T:",
    };

    let content_str = match &msg.content {
        Content::Single(content) => content_to_key(content),
        Content::Multi(contents) => contents
            .iter()
            .map(content_to_key)
            .collect::<Vec<_>>()
            .join("|"),
    };

    format!("{role_prefix}{content_str}")
}
