//! `TokenCounter` impl for `OpenAICompatibleProvider` and the
//! `estimate_tokens` free function (OpenAI-style ~4 chars per
//! token for English text).

use async_trait::async_trait;

use super::provider::OpenAICompatibleProvider;
use crate::token_counter::{TokenCounter, count_message_tokens};

pub(super) fn estimate_token_generic(
    text: &str,
    chars_per_token: f64,
) -> usize {
    (text.chars().count() as f64 / chars_per_token).ceil() as usize
}

/// Estimate tokens using OpenAI-style encoding (~4 chars per token for English text).
pub fn estimate_tokens(text: &str) -> usize {
    estimate_token_generic(text, 4.0)
}

#[async_trait]
impl TokenCounter for OpenAICompatibleProvider {
    fn count_text(&self, text: &str) -> usize {
        estimate_token_generic(text, 4.0)
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
        count_message_tokens(self, message, 4.0)
    }
}
