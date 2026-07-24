//! Shared helpers for the compaction sub-modules.
//!
//! These are private to the `compaction` module. They were
//! duplicated between `Compactor::truncate` (line ~615 of the old
//! `compactor.rs`) and the free `truncate_str` (line ~1142); keeping
//! them in a single private module also makes the boundary-safety
//! story auditable: every byte truncation in the compaction pipeline
//! now goes through one of these helpers, both of which delegate to
//! `synthia_core::cap_to_char_boundary` (the canonical UTF-8 safe
//! boundary primitive) for the actual `is_char_boundary` walk.

use synthia_provider::Message;

use crate::traits::extract_message_text;

/// Truncate `s` to at most `max_chars` Unicode code points and append
/// a `"..."` suffix when truncation occurred.
///
/// Char-based (not byte-based) so the output length is predictable
/// across scripts: a Chinese sentence and an English sentence with
/// the same `max_chars` produce the same number of characters in the
/// truncated output. The underlying iterator is over `char`s so the
/// operation is UTF-8 safe by construction (no slicing of multi-byte
/// sequences, no boundary walk needed).
///
/// If `s.chars().count() <= max_chars`, returns `s` unchanged.
pub(crate) fn truncate_to_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_chars).collect();
    format!("{}...", truncated)
}

/// Render a slice of messages as a single string of
/// `"[Role]: <text>"` lines joined by newlines. Empty message texts
/// are skipped so the output stays compact.
///
/// This is the canonical message → string conversion used by every
/// compaction path that needs to write the compacted result back
/// into a single text field (L1 summary body, L2 fallback content,
/// L3 fallback content, the L4 token-budget `compact_to_token_budget`
/// path, and the orchestrator's L2/L3 branches). Centralising it
/// here eliminates four duplicate inline implementations of the
/// same `format!("[{:?}]: {}", m.role, extract_message_text(m))`
/// expression that previously lived in `compactor.rs`.
pub(crate) fn messages_to_string(messages: &[Message]) -> String {
    messages
        .iter()
        .filter_map(|m| {
            let text = extract_message_text(m);
            if text.is_empty() {
                None
            } else {
                Some(format!("[{:?}]: {}", m.role, text))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Head-and-tail byte cap that fits within `max_bytes`, keeping a
/// `head_ratio` head, a single `… (N chars truncated) …` marker, and
/// a `(1 - head_ratio)` tail. The result is always valid UTF-8 and
/// `result.len() <= max_bytes` (subject to the marker contributing
/// a few bytes; the caller passes a value with the marker already in
/// mind).
///
/// Used to cap `<previous-summary>` anchor blocks so they don't grow
/// linearly across successive L1 compactions. The byte-based cut
/// goes through `synthia_core::cap_to_char_boundary` (re-imported
/// under the alias `truncate_to_boundary`) for the boundary walk so
/// multi-byte sequences never panic — previously the
/// `is_char_boundary` walk was open-coded here.
pub(crate) fn cap_to_head_tail(
    s: &str,
    max_bytes: usize,
    head_ratio: f64,
) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }

    // Reserve a fixed marker budget. The marker text is small (~30
    // chars) even for huge inputs (the digit count grows
    // logarithmically), so 64 bytes is a safe over-estimate that
    // keeps the algorithm simple.
    const MARKER_BUDGET: usize = 64;
    let usable = max_bytes.saturating_sub(MARKER_BUDGET);
    let head_budget = ((usable as f64) * head_ratio).floor() as usize;
    let tail_budget = usable.saturating_sub(head_budget);

    // Floor the head end down to a valid char boundary, via the
    // canonical UTF-8-safe helper.
    let mut head = s[..head_budget.min(s.len())].to_string();
    synthia_core::cap_to_char_boundary(&mut head, head_budget);

    // Ceil the tail start up to a valid char boundary so the tail
    // contains the requested budget's worth of bytes (rather than a
    // few bytes less due to a partial codepoint at the cut).
    let mut tail_start = s.len().saturating_sub(tail_budget);
    while tail_start < s.len() && !s.is_char_boundary(tail_start) {
        tail_start += 1;
    }
    let tail = &s[tail_start..];

    let head_len = head.len();
    let dropped = s.len() - head_len - tail.len();
    format!("{head}\n[... {dropped} chars truncated ...]\n{tail}")
}

#[cfg(test)]
mod tests {
    use synthia_provider::Message;

    use super::*;

    #[test]
    fn truncate_to_chars_short_input_passthrough() {
        assert_eq!(truncate_to_chars("hello", 10), "hello");
    }

    #[test]
    fn truncate_to_chars_long_input_with_suffix() {
        let s = "abcdefghij";
        let out = truncate_to_chars(s, 5);
        assert_eq!(out, "abcde...");
    }

    #[test]
    fn truncate_to_chars_unicode_char_counting() {
        // "你好世界" = 4 chars
        let s = "你好世界";
        assert_eq!(truncate_to_chars(s, 10), "你好世界");
        assert_eq!(truncate_to_chars(s, 2), "你好...");
    }

    #[test]
    fn messages_to_string_skips_empty_texts() {
        let msgs = vec![
            Message::user("hello"),
            Message::assistant(""),
            Message::user("world"),
        ];
        assert_eq!(messages_to_string(&msgs), "[User]: hello\n[User]: world");
    }

    #[test]
    fn cap_to_head_tail_short_input_passthrough() {
        let s = "short summary";
        assert_eq!(cap_to_head_tail(s, 100, 0.6), s);
    }

    #[test]
    fn cap_to_head_tail_long_input_keeps_marker() {
        // 8_000 ASCII chars = 8_000 bytes. max_bytes = 4_000.
        // → truncated with marker.
        let s: String = "a".repeat(8_000);
        let out = cap_to_head_tail(&s, 4_000, 0.6);
        assert!(
            out.contains("chars truncated"),
            "expected marker in output, got first 200 chars: {}",
            &out[..out.len().min(200)]
        );
        assert!(out.len() <= 4_000 + 64);
    }

    #[test]
    fn cap_to_head_tail_unicode_safe() {
        // Each "中" is 3 bytes. 5_000 reps = 15_000 bytes.
        // The byte-based cut must respect char boundaries.
        let s: String = "中".repeat(5_000);
        let out = cap_to_head_tail(&s, 4_000, 0.6);
        assert!(std::str::from_utf8(out.as_bytes()).is_ok());
        assert!(out.len() <= 4_000 + 64);
    }
}
