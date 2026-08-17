//! `TokenCounter` impl for `AnthropicProvider` and the
//! `estimate_tokens` free function (Anthropic-style ~3.5 chars per
//! token average).

use async_trait::async_trait;

use super::provider::AnthropicProvider;
use crate::token_counter::{TokenCounter, count_message_tokens};

pub(super) fn estimate_token_generic(
    text: &str,
    chars_per_token: f64,
) -> usize {
    (text.chars().count() as f64 / chars_per_token).ceil() as usize
}

/// Estimate tokens using Anthropic-style encoding (~3.5 chars per token average).
pub fn estimate_tokens(text: &str) -> usize {
    estimate_token_generic(text, 3.5)
}

#[async_trait]
impl TokenCounter for AnthropicProvider {
    fn count_text(&self, text: &str) -> usize {
        estimate_tokens(text)
    }

    fn count_image(
        &self,
        width: u32,
        height: u32,
        detail: crate::types::ImageDetail,
    ) -> usize {
        match detail {
            crate::types::ImageDetail::Low => 85,
            crate::types::ImageDetail::High => {
                let tiles_w = ((width as f64 / 512.0).ceil() as usize).max(1);
                let tiles_h = ((height as f64 / 512.0).ceil() as usize).max(1);
                170 + 85 * tiles_w * tiles_h
            }
            crate::types::ImageDetail::Auto => 170,
        }
    }

    fn count_message(&self, message: &crate::types::Message) -> usize {
        count_message_tokens(self, message, 3.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Empty string MUST return 0 — caller
    /// relies on this for short-circuit token
    /// budgeting on empty assistant output.
    #[test]
    fn estimate_token_generic_empty_string_returns_zero() {
        assert_eq!(estimate_token_generic("", 3.5), 0);
    }

    /// ASCII chars count as 1 char each (not
    /// byte count): "hello" = 5 chars / 3.5 =
    /// 2.
    #[test]
    fn estimate_token_generic_ascii_uses_char_count_not_byte_count() {
        assert_eq!(estimate_token_generic("hello", 3.5), 2);
        assert_eq!(estimate_token_generic("hello world", 3.5), 4);
    }

    /// CJK chars count as 1 char each even
    /// though they occupy 3 bytes in UTF-8.
    /// This is the canonical safe default —
    /// using `len()` (byte count) would
    /// double-count CJK.
    #[test]
    fn estimate_token_generic_cjk_uses_char_count_not_byte_count() {
        let cjk = "你好"; // 2 chars, 6 bytes UTF-8
        assert_eq!(cjk.len(), 6);
        assert_eq!(cjk.chars().count(), 2);
        // 2 / 3.5 = 0.571... → ceil = 1
        assert_eq!(estimate_token_generic(cjk, 3.5), 1);
    }

    /// `estimate_tokens` (Anthropic default
    /// of 3.5 chars/token) MUST produce the
    /// same result as
    /// `estimate_token_generic` with the
    /// same arg.
    #[test]
    fn estimate_tokens_matches_generic_with_3_5_ratio() {
        for text in ["", "hi", "the quick brown fox", "你好世界"] {
            assert_eq!(
                estimate_tokens(text),
                estimate_token_generic(text, 3.5),
                "estimate_tokens MUST match generic helper for: {text:?}"
            );
        }
    }

    /// `chars_per_token = 1.0` means every
    /// char is 1 token. Pin: 5 chars → 5
    /// tokens.
    #[test]
    fn estimate_token_generic_one_char_per_token_counts_chars_directly() {
        assert_eq!(estimate_token_generic("hello", 1.0), 5);
        assert_eq!(estimate_token_generic("你好", 1.0), 2);
    }

    /// `chars_per_token = 0.5` means each
    /// token holds 0.5 chars, so 1 token per
    /// 0.5 chars → ceil(1/0.5)=2 for "h".
    /// Verify the inverse direction
    /// separately from the default.
    #[test]
    fn estimate_token_generic_high_chars_per_token_floors_up() {
        // 5 chars / 10 = 0.5 → ceil = 1
        assert_eq!(estimate_token_generic("hello", 10.0), 1);
        // 50 chars / 10 = 5.0 → ceil = 5
        assert_eq!(estimate_token_generic("a".repeat(50).as_str(), 10.0), 5);
    }

    /// The helper MUST `ceil`, not round or
    /// floor, so a model never gets fewer
    /// tokens than the count actually needs.
    /// Pin: 4 chars / 3.5 = 1.14... → 2 (not
    /// 1).
    #[test]
    fn estimate_token_generic_uses_ceil_not_floor() {
        // 4 chars / 3.5 ≈ 1.1428 → ceil = 2
        assert_eq!(estimate_token_generic("abcd", 3.5), 2);
        // 3 chars / 3.5 ≈ 0.857 → ceil = 1
        assert_eq!(estimate_token_generic("abc", 3.5), 1);
        // 7 chars / 3.5 = 2.0 → ceil = 2
        assert_eq!(estimate_token_generic("abcdefg", 3.5), 2);
    }
}
