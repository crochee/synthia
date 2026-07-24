//! Byte-budget text truncation used by [`super::prompt::build_review_prompt`]
//! to fit entries into per-entry / per-message / per-tool token caps.
//!
//! The "tokens" argument is converted to bytes via a fixed 4-bytes-per-token
//! heuristic. When the content exceeds the budget, the helper keeps the
//! first `max_bytes / 2` bytes and the last `max_bytes / 2` bytes,
//! joining them with a `<truncated>` marker.

/// 截断文本到指定 token 数
pub(super) fn truncate_text(content: &str, max_tokens: usize) -> String {
    let max_bytes = max_tokens * 4;

    if content.len() <= max_bytes {
        return content.to_string();
    }

    let prefix_len = max_bytes / 2;
    let suffix_len = max_bytes / 2;

    let prefix = &content[..prefix_len];
    let suffix = &content[content.len() - suffix_len..];

    format!("{prefix}<truncated>{suffix}")
}
