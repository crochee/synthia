//! The five public entry points on [`super::core::Compactor`]:
//!
//! - [`compact`]: pick the right level, return a [`CompactionPart`].
//! - [`compact_with_provider`]: same as [`compact`] but Level 1 is
//!   allowed to use an LLM [`CompactionProvider`] to generate a
//!   real summary (falls back to the structured path on None /
//!   failure / empty result).
//! - [`compact_with_marker`]: wrap [`compact`] in a [`RangeMarker`]
//!   recording the message-range that was compacted.
//! - [`auto_select_level`]: pick a level based on the
//!   `current_tokens / budget_tokens` ratio (0 if under-budget,
//!   else 1 / 2 / 3 for ratio thresholds 1.5x / 3x).
//! - [`compact_to_token_budget`]: Stage-3 fallback that walks the
//!   message list newest → oldest, accumulating messages until
//!   adding the next one would exceed the target token budget.

use synthia_provider::Message;

use super::{
    super::util::messages_to_string,
    core::Compactor,
    levels::{
        level1_summary,
        level1_summary_with_provider,
        level2_truncate,
        level3_marker_only,
    },
};
use crate::{
    traits::estimate_message_tokens,
    types::{CompactionPart, ContextError, RangeMarker},
};

impl Compactor {
    /// Compact the given messages according to the configured level.
    pub fn compact(
        &self,
        messages: &[Message],
    ) -> Result<CompactionPart, ContextError> {
        let original_tokens = Self::estimate_tokens(messages);

        match self.level {
            1 => level1_summary(self, messages, original_tokens),
            2 => level2_truncate(self, messages, original_tokens),
            3 => level3_marker_only(self, messages, original_tokens),
            _ => {
                let content = messages_to_string(messages);
                Ok(CompactionPart {
                    content,
                    original_tokens,
                    compacted_tokens: original_tokens,
                })
            }
        }
    }

    /// Compact messages using an LLM provider for level-1 summary generation.
    ///
    /// When a provider is available and the level is 1, this method attempts to
    /// generate a real LLM summary. If the provider is None, fails, or the level
    /// is not 1, it falls back to the existing structured summary or other
    /// compaction strategies.
    ///
    /// `previous_summary`, when `Some(_)`, is forwarded to the LLM provider so
    /// the new summary is anchored to the prior one. The fallback path also
    /// embeds the previous summary in its structured output.
    pub async fn compact_with_provider(
        &self,
        messages: &[Message],
        provider: Option<&dyn super::super::level1::CompactionProvider>,
        previous_summary: Option<&str>,
    ) -> Result<CompactionPart, ContextError> {
        let original_tokens = Self::estimate_tokens(messages);

        match self.level {
            1 => {
                level1_summary_with_provider(
                    self,
                    messages,
                    original_tokens,
                    provider,
                    previous_summary,
                )
                .await
            }
            2 => level2_truncate(self, messages, original_tokens),
            3 => level3_marker_only(self, messages, original_tokens),
            _ => {
                let content = messages_to_string(messages);
                Ok(CompactionPart {
                    content,
                    original_tokens,
                    compacted_tokens: original_tokens,
                })
            }
        }
    }

    /// Compact a range of messages and return a RangeMarker.
    pub fn compact_with_marker(
        &self,
        messages: &[Message],
        start_index: usize,
        end_index: usize,
    ) -> Result<(CompactionPart, RangeMarker), ContextError> {
        let part = self.compact(messages)?;
        let marker = RangeMarker::new(start_index, end_index);
        Ok((part, marker))
    }

    /// Auto-select compaction level based on how far over budget we are.
    pub fn auto_select_level(
        &self,
        current_tokens: usize,
        budget_tokens: usize,
    ) -> usize {
        if current_tokens <= budget_tokens {
            return 0;
        }

        let ratio = current_tokens as f64 / budget_tokens as f64;

        if ratio > 3.0 {
            3
        } else if ratio > 1.5 {
            2
        } else {
            1
        }
    }

    /// Compact a range of messages to fit within a token budget.
    /// Used as a Stage 3 fallback when structured compaction still exceeds the budget.
    pub fn compact_to_token_budget(
        &self,
        messages: &[Message],
        target_tokens: usize,
    ) -> Result<CompactionPart, ContextError> {
        if messages.is_empty() {
            return Ok(CompactionPart {
                content: String::new(),
                original_tokens: 0,
                compacted_tokens: 0,
            });
        }

        let original_tokens = Self::estimate_tokens(messages);

        if original_tokens <= target_tokens {
            return Ok(CompactionPart {
                content: messages_to_string(messages),
                original_tokens,
                compacted_tokens: original_tokens,
            });
        }

        // Walk newest → oldest, accumulate until adding the next
        // message would exceed the budget.
        let mut tokens_so_far: usize = 0;
        let mut kept_messages: Vec<&Message> = Vec::new();

        for msg in messages.iter().rev() {
            let msg_tokens = estimate_message_tokens(msg);
            if tokens_so_far + msg_tokens > target_tokens {
                break;
            }
            tokens_so_far += msg_tokens;
            kept_messages.push(msg);
        }

        kept_messages.reverse();

        if kept_messages.is_empty() {
            let count = messages.len();
            let content =
                format!("[{} messages removed to fit token budget]", count);
            let compacted_tokens = Self::estimate_token_count(&content);
            return Ok(CompactionPart {
                content,
                original_tokens,
                compacted_tokens,
            });
        }

        // Dereference the `Vec<&Message>` to an owned `Vec<Message>`
        // so `messages_to_string` (which takes `&[Message]`) can
        // consume it.
        let kept: Vec<Message> =
            kept_messages.iter().copied().cloned().collect();
        let content = messages_to_string(&kept);
        let compacted_tokens = Self::estimate_token_count(&content);

        Ok(CompactionPart {
            content,
            original_tokens,
            compacted_tokens,
        })
    }
}
