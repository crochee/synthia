//! Private content-mutator helpers used by
//! [`super::truncate_messages`]:
//!
//! - `replace_first_text_anywhere` — the
//!   cleared-placeholder-injector that handles both Shape A
//!   (`ContentPart::ToolResult` with inner `Text`) and
//!   Shape B (`ContentPart::Text` top-level).
//! - `replace_first_in_tool_result` — the Shape A inner-text
//!   mutator.
//! - `set_msg_text` — the size-based-truncation
//!   `Message.content` writer (only handles Shape B because
//!   size-based truncation operates on extracted plain text).
//!
//! All three are `pub(super)` — they are not part of the
//! public API; they are the "how do we mutate a `Content`
//! in place" implementation detail.

use synthia_provider::{Content, ContentPart, Message, ToolResult};

/// Replace the first text-like field in `content` with `new_text`.
///
/// Handles both tool-result on-the-wire shapes that exist in this crate:
/// - `Content::Single(ContentPart::Text(t))` → set `t.text` (Shape B)
/// - `Content::Single(ContentPart::ToolResult(tr))` → set the first
///   `ContentPart::Text.text` inside `tr.content[]` (Shape A, the
///   Anthropic / OpenAI convention that `prune()` actually marks)
/// - `Content::Multi(parts)` → find the first `Text` or `ToolResult`
///   part in array order and apply the same replacement rule
///
/// Returns `true` if a replacement was made; `false` if no text-like
/// field was found (the caller treats this as a no-op — no panic, no
/// fallthrough to size-based truncation). This makes the helper safe
/// for `Content::Single(ContentPart::Image(_))` and similar variants
/// that legitimately carry no text.
#[allow(clippy::collapsible_match)]
pub(super) fn replace_first_text_anywhere(
    content: &mut Content,
    new_text: &str,
) -> bool {
    match content {
        Content::Single(part) => match part {
            ContentPart::Text(t) => {
                t.text = new_text.to_string();
                true
            }
            ContentPart::ToolResult(tr) => {
                replace_first_in_tool_result(tr, new_text)
            }
            _ => false,
        },
        Content::Multi(parts) => {
            for part in parts.iter_mut() {
                match part {
                    ContentPart::Text(t) => {
                        t.text = new_text.to_string();
                        return true;
                    }
                    ContentPart::ToolResult(tr) => {
                        if replace_first_in_tool_result(tr, new_text) {
                            return true;
                        }
                        // Continue scanning: this ToolResult had no
                        // inner text, so the placeholder belongs in a
                        // later part (or nowhere, returning false).
                    }
                    _ => {}
                }
            }
            false
        }
    }
}

/// Helper: replace the first `ContentPart::Text.text` inside
/// `tr.content[]` if any. Returns `true` on success, `false` when the
/// inner `content` array is empty or contains no text part.
pub(super) fn replace_first_in_tool_result(
    tr: &mut ToolResult,
    new_text: &str,
) -> bool {
    for part in tr.content.iter_mut() {
        if let ContentPart::Text(t) = part {
            t.text = new_text.to_string();
            return true;
        }
    }
    false
}

/// Replace the first Text part of `msg.content` with `new_text`.
/// If the content is a Single Text part, swap it; if it's Multi, replace
/// the first Text; otherwise leave it alone (the predicate caller's
/// responsibility is to only target messages where this will succeed).
pub(super) fn set_msg_text(msg: &mut Message, new_text: &str) {
    match &mut msg.content {
        Content::Single(part) => {
            if let ContentPart::Text(tc) = part {
                tc.text = new_text.to_string();
            }
        }
        Content::Multi(parts) => {
            for part in parts.iter_mut() {
                if let ContentPart::Text(tc) = part {
                    tc.text = new_text.to_string();
                    return;
                }
            }
        }
    }
}
