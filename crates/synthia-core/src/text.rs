//! UTF-8 safe string utilities.
//!
//! These helpers exist because `String::truncate` from the standard
//! library panics when the supplied index falls in the middle of a
//! multi-byte UTF-8 character. Several call sites in the workspace
//! enforce a length cap on user-controlled or third-party content
//! (web fetch response bodies, bash command output, tool result
//! strings, compaction summaries, etc.). Calling `truncate` directly
//! on that content is a latent panic. The helpers here are the safe
//! replacement.
//!
//! `synthia_core` is the right home for them: it is the foundational
//! crate that every domain crate (tool, context, session, …) already
//! depends on, so the helpers can be shared without a domain-level
//! dependency. They are pure string operations with no agent /
//! session / provider knowledge.

/// Truncate `s` to at most `max_bytes`, walking backward to the
/// nearest valid UTF-8 character boundary so we never panic on
/// multi-byte sequences.
///
/// # Contract
///
/// - `result.len() <= max_bytes` after the call.
/// - The result is guaranteed to be valid UTF-8.
/// - If `s.len() <= max_bytes` or `s` is empty, the call is a no-op.
/// - If `max_bytes == 0` and `s` is non-empty, `s` is cleared.
///
/// # Cost
///
/// `String::is_char_boundary` is O(1) in Rust, so the worst case walks
/// back at most 3 bytes (the longest UTF-8 leading byte is at most 4
/// bytes wide). Effectively constant-time.
pub fn cap_to_char_boundary(s: &mut String, max_bytes: usize) {
    if s.len() <= max_bytes {
        return;
    }
    if max_bytes == 0 {
        s.clear();
        return;
    }
    // Walk back from `max_bytes` to the nearest char boundary.
    let mut boundary = max_bytes;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    s.truncate(boundary);
}

/// Truncate `s` to at most `max_chars` Unicode scalar values, returning
/// the truncated slice and a flag that says whether truncation
/// actually happened. Char-count (not byte-count) is the contract so
/// the helper behaves correctly for multi-byte text (中文 / 日本語 /
/// 4-byte emoji).
///
/// # Contract
///
/// - `result.chars().count() <= max_chars` for the returned `String`.
/// - The result is guaranteed to be valid UTF-8.
/// - If `s.chars().count() <= max_chars`, returns `(s.to_string(), false)`.
/// - If `s.chars().count() > max_chars`, returns the first `max_chars`
///   characters and `true`.
/// - If `s` is empty, returns `(String::new(), false)`.
/// - If `max_chars == 0` and `s` is non-empty, returns `(String::new(), true)`.
///
/// # Why no ellipsis
///
/// The helper returns a `(String, bool)` so the caller decides
/// whether to append a marker — the marker is a presentation
/// concern, not a truncation one. Some call sites use `…`, others
/// use `[truncated N chars]`, others (the `s.chars().take(N).collect()`
/// inline sites in `synthia-mcp` / `synthia-session` / `synthia-steering`
/// / `synthia-attachment` / `synthia-harness`) keep the
/// raw truncated text. Pinning one shape here would force every
/// caller to do post-processing; the boolean is the contract.
///
/// # Cost
///
/// O(min(s.chars().count(), max_chars)). No allocations when the
/// input fits.
#[must_use]
pub fn truncate_chars(s: &str, max_chars: usize) -> (String, bool) {
    let total = s.chars().count();
    if total <= max_chars {
        return (s.to_string(), false);
    }
    let kept: String = s.chars().take(max_chars).collect();
    (kept, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== (a) Chinese 3-byte mid-character =====
    #[test]
    fn chinese_3byte_mid_character() {
        // "你好世界" = each char 3 bytes UTF-8, 12 bytes total.
        // Truncating to 7 bytes falls in the middle of the 3rd char
        // (bytes 6..9); should round down to 6, leaving "你好".
        let mut s = String::from("你好世界");
        cap_to_char_boundary(&mut s, 7);
        assert_eq!(s, "你好");
        assert!(std::str::from_utf8(s.as_bytes()).is_ok());
    }

    // ===== (b) Emoji 4-byte mid-character =====
    #[test]
    fn emoji_4byte_mid_character() {
        // 😀 = 4 bytes UTF-8 (F0 9F 98 80), 😀😀 = 8 bytes.
        // Truncating to 5 bytes falls in the middle of the 2nd emoji
        // (bytes 4..8); should round down to 4, leaving "😀".
        let mut s = String::from("😀😀");
        cap_to_char_boundary(&mut s, 5);
        assert_eq!(s, "😀");
        assert!(std::str::from_utf8(s.as_bytes()).is_ok());
    }

    // ===== (c) Mixed multibyte =====
    #[test]
    fn mixed_multibyte() {
        // "Hi你好😀" = "Hi"(2) + "你"(3) + "好"(3) + "😀"(4) = 12 bytes.
        // Truncating to 6 bytes: "Hi"(2) + "你"(3) = 5 bytes already,
        // adding any part of "好" would push past 6, so should round
        // down to 5, leaving "Hi你".
        let mut s = String::from("Hi你好😀");
        cap_to_char_boundary(&mut s, 6);
        assert_eq!(s, "Hi你");
        assert!(std::str::from_utf8(s.as_bytes()).is_ok());
    }

    // ===== (d) Boundary exact =====
    #[test]
    fn boundary_exact_no_adjustment() {
        // All ASCII, max_bytes == s.len(): no adjustment, no panic.
        let mut s = String::from("abc");
        cap_to_char_boundary(&mut s, 3);
        assert_eq!(s, "abc");
    }

    // ===== (e) Empty input =====
    #[test]
    fn empty_input_is_noop() {
        let mut s = String::new();
        cap_to_char_boundary(&mut s, 0);
        assert_eq!(s, "");
        let mut s = String::new();
        cap_to_char_boundary(&mut s, 1000);
        assert_eq!(s, "");
    }

    // ===== (f) All-ASCII =====
    #[test]
    fn all_ascii_truncates_to_max_bytes() {
        let mut s = String::from("Hello, World!");
        cap_to_char_boundary(&mut s, 5);
        assert_eq!(s, "Hello");
    }

    // ===== (g) Mid-multibyte truncate-to-zero =====
    #[test]
    fn mid_multibyte_truncate_to_zero() {
        // "中" = 3 bytes. max_bytes = 1 falls inside the char; should
        // round down to 0, leaving an empty string.
        let mut s = String::from("中");
        cap_to_char_boundary(&mut s, 1);
        assert_eq!(s, "");
        assert!(std::str::from_utf8(s.as_bytes()).is_ok());
    }

    // ===== (h) Truncate-no-op when s.len() <= max_bytes =====
    #[test]
    fn truncate_noop_when_under_max() {
        let mut s = String::from("你好");
        let original_len = s.len();
        cap_to_char_boundary(&mut s, 1000);
        assert_eq!(s, "你好");
        assert_eq!(s.len(), original_len);
    }

    // ===== Bonus: max_bytes = 0 on non-empty input =====
    #[test]
    fn max_bytes_zero_clears_non_empty() {
        let mut s = String::from("anything");
        cap_to_char_boundary(&mut s, 0);
        assert_eq!(s, "");
    }

    // ===== truncate_chars: char-based, no ellipsis =====

    /// `truncate_chars` reports `(verbatim, false)` when the input
    /// is shorter than the cap, so the caller can skip the
    /// post-processing path.
    #[test]
    fn truncate_chars_shorter_than_max_returns_verbatim() {
        let (kept, was_truncated) = truncate_chars("hi", 5);
        assert_eq!(kept, "hi");
        assert!(!was_truncated);

        let (kept, was_truncated) = truncate_chars("hello", 5);
        assert_eq!(kept, "hello");
        assert!(!was_truncated, "exact match must not be marked truncated");
    }

    /// `truncate_chars` flags truncation when the input is one
    /// char over the cap; the returned `String` is the first
    /// `max_chars` chars with no marker (the caller adds the
    /// marker).
    #[test]
    fn truncate_chars_one_over_max_is_truncated() {
        let (kept, was_truncated) = truncate_chars("hello!", 5);
        assert_eq!(kept, "hello");
        assert!(was_truncated);
    }

    /// Empty input is always `(empty, false)` — there is nothing
    /// to truncate.
    #[test]
    fn truncate_chars_empty_string_returns_empty() {
        let (kept, was_truncated) = truncate_chars("", 0);
        assert_eq!(kept, "");
        assert!(!was_truncated);

        let (kept, was_truncated) = truncate_chars("", 5);
        assert_eq!(kept, "");
        assert!(!was_truncated);
    }

    /// `max_chars == 0` on non-empty input is the corner case the
    /// server's `truncate("x", 0) == "…"` test pins: the helper
    /// returns `(empty, true)` and the caller appends the
    /// ellipsis.
    #[test]
    fn truncate_chars_zero_max_with_nonempty_marks_truncated() {
        let (kept, was_truncated) = truncate_chars("x", 0);
        assert_eq!(kept, "");
        assert!(was_truncated, "an ellipsis-only result must mark truncated");
    }

    /// Char-count, not byte-count. 中文 is 2 chars but 6 bytes;
    /// `max_chars=2` must return verbatim, not garbage.
    #[test]
    fn truncate_chars_counts_chars_not_bytes_for_multibyte_text() {
        let (kept, was_truncated) = truncate_chars("中文", 2);
        assert_eq!(kept, "中文");
        assert!(!was_truncated);

        let (kept, was_truncated) = truncate_chars("中文!", 2);
        assert_eq!(kept, "中文");
        assert!(was_truncated);

        let (kept, was_truncated) = truncate_chars("日本語テスト", 3);
        assert_eq!(kept, "日本語");
        assert!(was_truncated);
    }

    /// 4-byte emoji: 😀 is 1 char (4 bytes UTF-8). Char-count
    /// keeps the boundary at the right place; byte-count would
    /// truncate mid-emoji.
    #[test]
    fn truncate_chars_handles_4byte_emoji() {
        let (kept, was_truncated) = truncate_chars("😀😀", 1);
        assert_eq!(kept, "😀");
        assert!(was_truncated);
    }
}
