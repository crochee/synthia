//! Token-count estimation heuristics.
//!
//! `estimate_token_count` is a coarse approximation used by
//! [`synthia_provider::token_counter`](crate::) when no
//! provider-native tokenizer is available (e.g. for pre-flight
//! budget checks before the request reaches the upstream API).
//!
//! The formula combines a 4-byte-per-token ratio for ASCII
//! content with a 1.5-characters-per-token ratio for CJK, and
//! adds a 5% overhead to account for system prompt and tool
//! definitions. It is intentionally cheap — provider-native
//! BPE tokenizers are used wherever accurate counts matter.

/// Approximate token count for `text`.
///
/// Uses a coarse heuristic (4 ASCII bytes per token, 1.5 CJK
/// characters per token) plus a 5% overhead. Provider-native
/// BPE tokenizers should be preferred when accuracy matters
/// — this helper exists for pre-flight budget checks only.
pub fn estimate_token_count(text: &str) -> usize {
    let mut ascii_count: usize = 0;
    let mut cjk_count: usize = 0;
    for ch in text.chars() {
        if is_cjk(ch) {
            cjk_count += 1;
        } else {
            ascii_count += ch.len_utf8();
        }
    }
    let text_tokens =
        (ascii_count as f64 / 4.0 + cjk_count as f64 / 1.5) as usize;
    let overhead = (text_tokens as f64 * 0.05) as usize;
    text_tokens + overhead
}

fn is_cjk(ch: char) -> bool {
    matches!(ch,
        '\u{4E00}'..='\u{9FFF}' |
        '\u{3400}'..='\u{4DBF}' |
        '\u{20000}'..='\u{2A6DF}' |
        '\u{2A700}'..='\u{2B73F}' |
        '\u{2B740}'..='\u{2B81F}' |
        '\u{2B820}'..='\u{2CEAF}' |
        '\u{F900}'..='\u{FAFF}' |
        '\u{2F800}'..='\u{2FA1F}' |
        '\u{3000}'..='\u{303F}' |
        '\u{3040}'..='\u{309F}' |
        '\u{30A0}'..='\u{30FF}' |
        '\u{AC00}'..='\u{D7AF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Empty input returns 0 (no panic, no
    /// overhead on zero). Pin the contract.
    #[test]
    fn estimate_token_count_empty_string_returns_zero() {
        assert_eq!(estimate_token_count(""), 0);
    }

    /// ASCII-only text: each 4 ASCII chars
    /// count as 1 token, with a 5% overhead
    /// applied at the end. Pin the exact
    /// formula.
    #[test]
    fn estimate_token_count_ascii_only_uses_4_chars_per_token() {
        assert_eq!(estimate_token_count("abcd"), 1);
        assert_eq!(estimate_token_count("abcdefgh"), 2);
        assert_eq!(estimate_token_count(&"a".repeat(100)), 26);
    }

    /// CJK-only text: each character counts as
    /// 1, divided by 1.5, with a 5% overhead.
    /// Pin the exact formula and the
    /// `ch.len_utf8()` (3 for CJK) bypass.
    #[test]
    fn estimate_token_count_cjk_only_uses_1_5_chars_per_token() {
        assert_eq!(estimate_token_count("中文中"), 2);
        assert_eq!(estimate_token_count("中文中文中文"), 4);
    }

    /// CJK chars are counted by CHARACTER, not
    /// by UTF-8 byte length. Pin the contract.
    #[test]
    fn estimate_token_count_cjk_char_count_not_byte_count() {
        let one_cjk = estimate_token_count("中");
        assert_eq!(one_cjk, 0, "1 CJK char should round down to 0 tokens");
    }

    /// Mixed ASCII + CJK: the two counts are
    /// summed BEFORE the per-cast to `usize`.
    /// Pin the contract.
    #[test]
    fn estimate_token_count_mixed_ascii_and_cjk_sums_both() {
        assert_eq!(estimate_token_count("abc中文"), 2);
        assert_eq!(estimate_token_count("abcdefgh中文中文中"), 5);
    }

    /// Whitespace and ASCII punctuation count
    /// as ASCII (their UTF-8 length is 1).
    #[test]
    fn estimate_token_count_treats_whitespace_as_ascii() {
        assert_eq!(estimate_token_count("a b c"), 1);
    }

    /// Unicode non-CJK multi-byte (e.g. emoji)
    /// count as ASCII bytes.
    #[test]
    fn estimate_token_count_emoji_counts_as_ascii_bytes() {
        assert_eq!(estimate_token_count("🚀"), 1);
    }

    /// `is_cjk` MUST recognize every documented
    /// range's lower bound.
    #[test]
    fn is_cjk_inclusive_lower_bound_of_every_range() {
        assert!(is_cjk('\u{4E00}'));
        assert!(is_cjk('\u{3400}'));
        assert!(is_cjk('\u{20000}'));
        assert!(is_cjk('\u{2A700}'));
        assert!(is_cjk('\u{2B740}'));
        assert!(is_cjk('\u{2B820}'));
        assert!(is_cjk('\u{F900}'));
        assert!(is_cjk('\u{2F800}'));
        assert!(is_cjk('\u{3000}'));
        assert!(is_cjk('\u{3040}'));
        assert!(is_cjk('\u{30A0}'));
        assert!(is_cjk('\u{AC00}'));
    }

    /// `is_cjk` MUST recognize every documented
    /// range's upper bound.
    #[test]
    fn is_cjk_inclusive_upper_bound_of_every_range() {
        assert!(is_cjk('\u{9FFF}'));
        assert!(is_cjk('\u{4DBF}'));
        assert!(is_cjk('\u{2A6DF}'));
        assert!(is_cjk('\u{2B73F}'));
        assert!(is_cjk('\u{2B81F}'));
        assert!(is_cjk('\u{2CEAF}'));
        assert!(is_cjk('\u{FAFF}'));
        assert!(is_cjk('\u{2FA1F}'));
        assert!(is_cjk('\u{303F}'));
        assert!(is_cjk('\u{309F}'));
        assert!(is_cjk('\u{30FF}'));
        assert!(is_cjk('\u{D7AF}'));
    }

    /// `is_cjk` MUST return false for codepoints
    /// outside each range.
    #[test]
    fn is_cjk_returns_false_for_codepoints_outside_ranges() {
        assert!(!is_cjk('\u{4DFF}'));
        assert!(!is_cjk('\u{33FF}'));
        assert!(!is_cjk('A'));
        assert!(!is_cjk('\u{ABFF}'));
        assert!(!is_cjk('a'));
        assert!(!is_cjk('1'));
    }

    /// `is_cjk` classifies purely by codepoint.
    #[test]
    fn is_cjk_classifies_by_codepoint_not_byte_length() {
        assert!(!is_cjk('A'));
        assert!(!is_cjk('z'));
        assert!(is_cjk('中'));
        assert!(is_cjk('日'));
    }
}
