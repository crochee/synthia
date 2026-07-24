//! Per-message truncation entry point —
//! [`truncate_messages`] — plus the public
//! [`cleared_placeholder`] helper.
//!
//! The "cleared placeholder" branch (triggered when
//! `msg.tool_result_cleared_at` is set) is intentionally
//! handled BEFORE the role predicate + size-based
//! truncation: prune marks a tool-result message as
//! cleared so the LLM-visible rendering is short, and
//! any size-based truncation running on the (still
//! intact) original `content` would defeat that
//! guarantee. P8 says "transform, never lose", so the
//! `content` field stays intact for the event log /
//! on-disk storage; only the LLM-visible text is
//! replaced.

use chrono::{DateTime, Utc};
use synthia_provider::Message;

use super::{
    text::{replace_first_text_anywhere, set_msg_text},
    truncate_output::truncate_output,
    types::{TruncateConfig, TruncatedResult},
};

/// Apply `truncate_output` to every message for which `role_predicate`
/// returns `true`. Order, role, and count of messages are preserved; only
/// the textual `content` of affected messages is replaced.
///
/// Returns one `TruncatedResult` per truncated message (in slice order).
pub fn truncate_messages(
    messages: &mut [Message],
    cfg: &TruncateConfig,
    role_predicate: impl Fn(&Message) -> bool,
) -> Vec<TruncatedResult> {
    let mut results = Vec::new();
    for msg in messages.iter_mut() {
        // `tool_result_cleared_at` is set by `synthia_context::pruning::prune`
        // to mark that the original tool-result content has been pushed out
        // of the protected tail. The LLM-visible rendering MUST see a
        // placeholder instead of the (potentially huge) original payload, so
        // we replace the text with a short marker before any size-based
        // truncation can run on it. The original `content` is left intact
        // in the struct so the event log / on-disk storage still carries it
        // (P8: transform, never lose).
        //
        // The previous implementation used `msg.content.extract_text()` as
        // a gate, which only returns `Some` for the legacy
        // `ContentPart::Text` shape (Shape B) and silently misses the
        // `ContentPart::ToolResult` shape (Shape A) that `prune()` actually
        // marks. The new helper dispatches on the content variant and
        // returns `false` for non-text variants (e.g. `ImageContent`) so
        // we never panic and never fall through to size-based truncation
        // for a cleared message.
        let cleared_at = msg.tool_result_cleared_at;
        if let Some(at) = cleared_at {
            let marker = cleared_placeholder(at);
            let _replaced =
                replace_first_text_anywhere(&mut msg.content, &marker);
            continue;
        }
        if !role_predicate(msg) {
            continue;
        }
        let Some(text) = msg.content.extract_text() else {
            continue;
        };
        if text.len() <= cfg.max_bytes {
            continue;
        }
        let result = truncate_output(&text, cfg);
        set_msg_text(msg, &result.output);
        results.push(result);
    }
    results
}

/// Render the LLM-visible placeholder for a message whose
/// `tool_result_cleared_at` timestamp is set. The format matches
/// the prune-idempotent-marker spec:
/// `"[Old tool result content cleared at {ISO8601_timestamp}]"`.
pub fn cleared_placeholder(at: DateTime<Utc>) -> String {
    format!("[Old tool result content cleared at {}]", at.to_rfc3339())
}
