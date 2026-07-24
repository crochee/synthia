//! Types for the protection zone system.

use synthia_provider::{Message, Role};

use crate::traits::estimate_message_tokens;

#[derive(Debug, Clone)]
pub struct CompactionBoundary {
    pub protected_start_index: usize,
    pub compact_end_index: usize,
    pub protected_tokens: usize,
    pub compactable_tokens: usize,
}

/// Protection zone configuration for context compaction.
/// Preserves recent N rounds and enforces a minimum token budget (30-40% of total).
#[derive(Debug, Clone)]
pub struct ProtectionZone {
    /// Minimum number of recent conversation rounds to protect
    pub min_rounds: usize,
    /// Ratio of total tokens that must be protected (e.g., 0.35 for 35%)
    pub token_ratio: f64,
}

impl Default for ProtectionZone {
    fn default() -> Self {
        Self {
            min_rounds: 3,
            token_ratio: 0.35,
        }
    }
}

impl ProtectionZone {
    pub fn new(min_rounds: usize, token_ratio: f64) -> Self {
        Self {
            min_rounds,
            token_ratio,
        }
    }

    /// Calculate the protection boundary: the token index below which messages are safe to compact.
    /// Returns the boundary as a token count.
    ///
    /// The boundary is the larger of:
    /// - The cumulative token count of the most recent `min_rounds` rounds
    /// - `total_tokens * token_ratio` (minimum protected token budget)
    pub fn calculate_boundary(
        &self,
        messages: &[Message],
        total_tokens: usize,
    ) -> usize {
        let min_budget = ((total_tokens as f64) * self.token_ratio) as usize;
        let recent_round_tokens =
            Self::count_recent_round_tokens(messages, self.min_rounds);
        recent_round_tokens.max(min_budget)
    }

    /// Count the total tokens in the most recent N conversation rounds.
    /// A "round" is a user message + assistant response pair.
    pub fn count_recent_round_tokens(
        messages: &[Message],
        rounds: usize,
    ) -> usize {
        if messages.is_empty() || rounds == 0 {
            return 0;
        }

        let mut token_count: usize = 0;
        let mut round_count: usize = 0;
        let mut i = messages.len();

        while i > 0 && round_count < rounds {
            i -= 1;
            token_count += estimate_message_tokens(&messages[i]);

            if matches!(messages[i].role, Role::User) {
                round_count += 1;
            }
        }

        token_count
    }

    /// Fetch the last N rounds of conversation from back to front.
    /// Returns messages starting from the boundary index to the end.
    pub fn get_recent_messages<'a>(
        &self,
        messages: &'a [Message],
        n_rounds: usize,
    ) -> Vec<&'a Message> {
        if messages.is_empty() || n_rounds == 0 {
            return vec![];
        }

        // First pass: find the indices of all user messages
        let user_indices: Vec<usize> = messages
            .iter()
            .enumerate()
            .filter(|(_, m)| matches!(m.role, Role::User))
            .map(|(i, _)| i)
            .collect();

        if user_indices.is_empty() {
            return vec![];
        }

        // We want the last N user messages (and everything after them)
        let n = n_rounds.min(user_indices.len());
        // The boundary is the index of the (total_users - N)th user message
        let start_user_idx = user_indices[user_indices.len() - n];

        messages[start_user_idx..].iter().collect()
    }

    /// Fetch the last N rounds, returning owned copies.
    pub fn get_recent_messages_owned(
        &self,
        messages: &[Message],
        n_rounds: usize,
    ) -> Vec<Message> {
        self.get_recent_messages(messages, n_rounds)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Calculate the compaction boundary excluding the last N checkpoints from compression.
    /// Returns which message indices are safe to compact vs must be protected.
    pub fn calculate_compaction_boundary(
        &self,
        messages: &[Message],
        checkpoint_indices: &[usize],
    ) -> CompactionBoundary {
        if messages.is_empty() {
            return CompactionBoundary {
                protected_start_index: 0,
                compact_end_index: 0,
                protected_tokens: 0,
                compactable_tokens: 0,
            };
        }

        let total_tokens: usize =
            messages.iter().map(estimate_message_tokens).sum();
        let min_protected_tokens =
            ((total_tokens as f64) * self.token_ratio) as usize;

        let mut protected_start = messages.len();

        for &checkpoint_idx in checkpoint_indices {
            if checkpoint_idx < messages.len()
                && checkpoint_idx < protected_start
            {
                protected_start = checkpoint_idx;
            }
        }

        if checkpoint_indices.is_empty() {
            protected_start = self.calculate_round_boundary(messages);
        }

        let tokens_from_start: usize = messages[..protected_start]
            .iter()
            .map(estimate_message_tokens)
            .sum();

        let tokens_protected = total_tokens.saturating_sub(tokens_from_start);
        let effective_protected_tokens =
            tokens_protected.max(min_protected_tokens);
        let compactable_tokens =
            total_tokens.saturating_sub(effective_protected_tokens);

        CompactionBoundary {
            protected_start_index: protected_start,
            compact_end_index: protected_start,
            protected_tokens: effective_protected_tokens,
            compactable_tokens,
        }
    }

    fn calculate_round_boundary(&self, messages: &[Message]) -> usize {
        if messages.is_empty() || self.min_rounds == 0 {
            return 0;
        }

        let user_indices: Vec<usize> = messages
            .iter()
            .enumerate()
            .filter(|(_, m)| matches!(m.role, Role::User))
            .map(|(i, _)| i)
            .collect();

        if user_indices.is_empty() {
            return 0;
        }

        let n = self.min_rounds.min(user_indices.len());
        user_indices[user_indices.len() - n]
    }

    pub fn calculate_compaction_boundary_simple(
        &self,
        messages: &[Message],
    ) -> CompactionBoundary {
        if messages.is_empty() {
            return CompactionBoundary {
                protected_start_index: 0,
                compact_end_index: 0,
                protected_tokens: 0,
                compactable_tokens: 0,
            };
        }

        let total_tokens: usize =
            messages.iter().map(estimate_message_tokens).sum();
        let min_protected_tokens =
            ((total_tokens as f64) * self.token_ratio) as usize;

        let user_indices: Vec<usize> = messages
            .iter()
            .enumerate()
            .filter(|(_, m)| matches!(m.role, Role::User))
            .map(|(i, _)| i)
            .collect();

        let n = self.min_rounds.min(user_indices.len());
        let protected_start = if user_indices.is_empty() {
            0
        } else {
            user_indices[user_indices.len() - n]
        };

        let tokens_up_to_protected: usize = messages[..protected_start]
            .iter()
            .map(estimate_message_tokens)
            .sum();

        let protected_tokens = total_tokens
            .saturating_sub(tokens_up_to_protected)
            .max(min_protected_tokens);
        let compactable_tokens = total_tokens.saturating_sub(protected_tokens);

        CompactionBoundary {
            protected_start_index: protected_start,
            compact_end_index: protected_start,
            protected_tokens,
            compactable_tokens,
        }
    }
}
