//! The [`Compactor`] struct itself, plus its `new` / `with_limits`
//! constructors and the two private `estimate_token_count` /
//! `estimate_tokens` helpers used by every level implementation.
//!
//! Per-level implementations live in [`super::levels`]; the public
//! dispatch entry points live in [`super::dispatch`].

use synthia_provider::Message;

/// Three-level degradation compaction strategy.
///
/// Level 1: LLM summary - Generate an AI-powered summary of the messages
/// Level 2: Structured truncation - Keep tool call inputs and first line of outputs
/// Level 3: Marker-only - Retain only call-completed markers
pub struct Compactor {
    pub(crate) level: usize,
    pub(crate) max_input_length: usize,
    pub(crate) max_output_lines: usize,
}

impl Compactor {
    pub fn new(level: usize) -> Self {
        Self {
            level,
            max_input_length: 500,
            max_output_lines: 1,
        }
    }

    pub fn with_limits(
        mut self,
        max_input_length: usize,
        max_output_lines: usize,
    ) -> Self {
        self.max_input_length = max_input_length;
        self.max_output_lines = max_output_lines;
        self
    }

    /// Estimate token count for a plain string (rounded up to the
    /// nearest 4-character group). Used to compute
    /// `compacted_tokens` in [`super::super::types::CompactionPart`].
    pub(crate) fn estimate_token_count(s: &str) -> usize {
        s.chars().count().div_ceil(4)
    }

    /// Estimate the total token count of a message slice by summing
    /// [`crate::traits::estimate_message_tokens`] per message.
    pub(crate) fn estimate_tokens(messages: &[Message]) -> usize {
        use crate::traits::estimate_message_tokens;
        messages.iter().map(estimate_message_tokens).sum()
    }
}
