//! Protection zone calculation: decides which message indices are safe to compact.

use synthia_provider::{Message, Role};

use super::fallback::estimate_tokens;
use crate::traits::estimate_message_tokens;

/// Calculate the protection zone: the range of message indices that should NOT be compacted.
///
/// Protects:
/// - Recent N conversation rounds (user message + assistant response pairs)
/// - At least 30-40% of total token budget (whichever is larger)
///
/// Returns `(protected_start_index, protected_end_index)`.
/// Messages with indices in `[protected_start_index, protected_end_index)` must not be compacted.
pub fn calculate_protection_zone(
    messages: &[Message],
    min_rounds: usize,
    token_budget: usize,
) -> (usize, usize) {
    if messages.is_empty() || min_rounds == 0 {
        return (0, 0);
    }

    let total_tokens = estimate_tokens(messages);
    let min_protected_tokens = ((total_tokens as f64) * 0.35)
        .max((token_budget as f64) * 0.35)
        as usize;

    // Find all user message indices
    let user_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| matches!(m.role, Role::User))
        .map(|(i, _)| i)
        .collect();

    if user_indices.is_empty() {
        return (0, 0);
    }

    // Determine the start index based on recent N rounds
    let n = min_rounds.min(user_indices.len());
    let round_start = user_indices[user_indices.len() - n];

    // Determine the start index based on token budget
    let mut token_start = messages.len();
    let mut cumulative = 0;
    for i in (0..messages.len()).rev() {
        cumulative += estimate_message_tokens(&messages[i]);
        if cumulative > min_protected_tokens {
            token_start = i + 1;
            break;
        }
        token_start = i;
    }

    // Take the earlier start (larger protected zone)
    let protected_start = round_start.min(token_start);

    (protected_start, messages.len())
}
