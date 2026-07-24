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
