use crate::types::{ImageDetail, Message};

/// Trait for estimating token counts across different LLM providers.
///
/// Each provider implements its own token counting logic based on their
/// tokenization approach.
pub trait TokenCounter: Send + Sync {
    /// Estimate tokens for a text string.
    fn count_text(&self, text: &str) -> usize;

    /// Estimate tokens for an image based on dimensions and detail level.
    ///
    /// For "low" detail: fixed 85 tokens.
    /// For "high" detail: 170 tokens + 85 * ceil(width/512) * ceil(height/512) tokens per tile.
    /// Reference: OpenAI vision pricing model.
    fn count_image(
        &self,
        width: u32,
        height: u32,
        detail: ImageDetail,
    ) -> usize;

    /// Estimate tokens for a complete message including all content parts.
    fn count_message(&self, message: &Message) -> usize;
}

/// Estimate tokens using a characters-per-token ratio.
pub(crate) fn estimate_tokens(text: &str, chars_per_token: f64) -> usize {
    if text.is_empty() {
        return 0;
    }
    (text.len() as f64 / chars_per_token).ceil() as usize
}

/// Count tokens in a message using the provided counter and chars-per-token ratio.
pub(crate) fn count_message_tokens(
    counter: &dyn TokenCounter,
    message: &Message,
    chars_per_token: f64,
) -> usize {
    let mut total = 0;
    for part in &message.content {
        match part {
            crate::types::ContentPart::Text(tc) => {
                total += counter.count_text(&tc.text);
            }
            crate::types::ContentPart::Image(ic) => {
                total += counter.count_image(
                    0,
                    0,
                    ic.detail.clone().unwrap_or(ImageDetail::Low),
                );
            }
            crate::types::ContentPart::ToolUse(tu) => {
                total += estimate_tokens(&tu.name, chars_per_token);
                if let Ok(json) = serde_json::to_string(&tu.input) {
                    total += estimate_tokens(&json, chars_per_token);
                }
            }
            crate::types::ContentPart::ToolResult(tr) => {
                for inner in &tr.content {
                    if let crate::types::ContentPart::Text(tc) = inner {
                        total += counter.count_text(&tc.text);
                    }
                }
            }
            crate::types::ContentPart::Reasoning(tc) => {
                total += counter.count_text(&tc.text);
            }
            crate::types::ContentPart::Resource(link) => {
                total += estimate_tokens(&link.uri, chars_per_token);
                total += estimate_tokens(&link.name, chars_per_token);
            }
            crate::types::ContentPart::Audio(_) => {
                // Audio tokens depend on provider-specific encoding, estimate conservatively
                total += 100;
            }
        }
    }
    total
}

/// Estimate tokens for a list of messages by **text-only**
/// extraction then summing per-message char-based
/// estimates. Returns `0` for an empty `messages` slice
/// (no allocation, no work).
///
/// Scope: this helper is the cheap, conservative estimate
/// used by the high-level observability / quota layers
/// where the caller only needs a rough magnitude. For
/// provider-accurate counts (handling images, tool calls,
/// audio, etc.) use
/// [`crate::token_counter::TokenCounter::count_message`]
/// or [`crate::token_counter::count_message_tokens`]
/// directly — those understand every `ContentPart`
/// variant. Mixing the two levels here would require a
/// `TokenCounter` instance and is intentionally out of
/// scope for this estimator.
pub fn estimate_messages_token_count(
    messages: &[crate::types::Message],
) -> usize {
    if messages.is_empty() {
        return 0;
    }
    messages
        .iter()
        .map(|m| {
            let text = match &m.content {
                crate::types::Content::Single(part) => {
                    part.text().map(|t| t.to_string()).unwrap_or_default()
                }
                crate::types::Content::Multi(parts) => parts
                    .iter()
                    .filter_map(|p| p.text().map(|t| t.to_string()))
                    .collect::<Vec<_>>()
                    .join(""),
            };
            synthia_core::token::estimate_token_count(&text)
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCounter {
        chars_per_token: f64,
    }

    impl TestCounter {
        fn new(chars_per_token: f64) -> Self {
            Self { chars_per_token }
        }
    }

    impl TokenCounter for TestCounter {
        fn count_text(&self, text: &str) -> usize {
            estimate_tokens(text, self.chars_per_token)
        }

        fn count_image(
            &self,
            width: u32,
            height: u32,
            detail: ImageDetail,
        ) -> usize {
            match detail {
                ImageDetail::Low => 85,
                ImageDetail::High => {
                    let tiles_w =
                        ((width as f64 / 512.0).ceil() as usize).max(1);
                    let tiles_h =
                        ((height as f64 / 512.0).ceil() as usize).max(1);
                    170 + 85 * tiles_w * tiles_h
                }
                ImageDetail::Auto => 170,
            }
        }

        fn count_message(&self, message: &Message) -> usize {
            count_message_tokens(self, message, self.chars_per_token)
        }
    }

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens("", 4.0), 0);
    }

    #[test]
    fn test_estimate_tokens_english_text() {
        // "Hello world" = 11 chars, ~3 tokens at 4 chars/token
        let tokens = estimate_tokens("Hello world", 4.0);
        assert_eq!(tokens, 3);
    }

    #[test]
    fn test_estimate_tokens_code() {
        // 60 chars of code at 2 chars/token = 30 tokens
        let code =
            "fn main() { println!(\"Hello\"); let x = 42; let y = x + 1; }";
        let tokens = estimate_tokens(code, 2.0);
        assert_eq!(tokens, 30);
    }

    #[test]
    fn test_image_low_detail() {
        let counter = TestCounter::new(4.0);
        assert_eq!(counter.count_image(100, 100, ImageDetail::Low), 85);
    }

    #[test]
    fn test_image_high_detail_single_tile() {
        let counter = TestCounter::new(4.0);
        // 512x512 = 1 tile = 170 + 85 * 1 * 1 = 255
        assert_eq!(counter.count_image(512, 512, ImageDetail::High), 255);
    }

    #[test]
    fn test_image_high_detail_multiple_tiles() {
        let counter = TestCounter::new(4.0);
        // 1024x1024 = 2x2 = 4 tiles = 170 + 85 * 4 = 510
        assert_eq!(counter.count_image(1024, 1024, ImageDetail::High), 510);
    }

    #[test]
    fn test_image_high_detail_non_standard() {
        let counter = TestCounter::new(4.0);
        // 600x400 = ceil(600/512) * ceil(400/512) = 2 * 1 = 2 tiles
        // = 170 + 85 * 2 = 340
        assert_eq!(counter.count_image(600, 400, ImageDetail::High), 340);
    }

    #[test]
    fn test_count_message_text_only() {
        let counter = TestCounter::new(4.0);
        let msg = Message::user("Hello world");
        let tokens = counter.count_message(&msg);
        assert_eq!(tokens, 3); // "Hello world" = 11 chars / 4 = 2.75, ceil = 3
    }

    #[test]
    fn test_count_message_complex() {
        let counter = TestCounter::new(4.0);
        let msg = Message::user("Test message with some length");
        let tokens = counter.count_message(&msg);
        // 30 chars / 4 = 7.5, ceil = 8
        assert_eq!(tokens, 8);
    }

    /// `estimate_messages_token_count` MUST return `0` for
    /// an empty `messages` slice. This pins the no-op
    /// behavior so a future refactor (e.g. changing the
    /// iterator chain to `.filter(|m| !m.content.is_empty())`)
    /// doesn't accidentally allocate or do work for the
    /// common "agent has not yet emitted any messages"
    /// observability poll.
    #[test]
    fn estimate_messages_token_count_is_zero_for_empty_slice() {
        let empty: &[Message] = &[];
        assert_eq!(estimate_messages_token_count(empty), 0);
    }

    /// `estimate_messages_token_count` is a TEXT-ONLY
    /// estimator: a message containing only an image must
    /// contribute `0` tokens to the total estimate. This
    /// is intentional — the high-level estimator is a
    /// rough magnitude gauge for quota dashboards, not a
    /// provider-accurate count. Provider-accurate counts
    /// (with image / audio / tool-call awareness) MUST go
    /// through [`TokenCounter::count_message`] or
    /// [`count_message_tokens`] — those are tested
    /// separately above and handle every `ContentPart`
    /// variant. Pinning this expectation here prevents a
    /// future refactor from silently turning the helper
    /// into a partial-implementation count that
    /// under-counts (e.g. summing 0 for images and then
    /// pretending the total is correct).
    #[test]
    fn estimate_messages_token_count_is_zero_for_image_only_message() {
        use crate::types::{Content, ContentPart, ImageContent, ImageDetail};
        let msg = Message {
            role: crate::types::Role::User,
            content: Content::Single(ContentPart::Image(ImageContent {
                data: "BASE64DATA".to_string(),
                mime_type: "image/png".to_string(),
                detail: Some(ImageDetail::High),
            })),
            tool_call_id: None,
            name: None,
            ..Default::default()
        };
        // High-detail image — provider would still charge
        // some tokens, but this helper is text-only, so
        // it returns 0.
        assert_eq!(estimate_messages_token_count(&[msg]), 0);
    }
}
