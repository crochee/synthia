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
}
