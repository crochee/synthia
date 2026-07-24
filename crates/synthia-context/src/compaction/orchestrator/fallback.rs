//! L1 → L2 → L3 fallback chain and `apply_compaction` write-back builder.

use synthia_provider::Message;

use super::result::CompactionResult;
use crate::{
    compaction::{
        level1::{CompactionProvider, compact_level1},
        level2::compact_level2,
        level3::compact_level3,
        util::messages_to_string,
    },
    traits::estimate_message_tokens,
    types::{ContextError, SummaryMessage},
};

/// Apply three-level degradation compaction and return a result for session write-back.
///
/// Tries Level 1 (LLM summary), falls through to Level 2 (structured truncation),
/// then to Level 3 (marker-only) based on token budget.
///
/// `previous_summary`, when `Some(_)`, is forwarded to L1 so the LLM
/// (or the structured fallback) anchors the new summary on top of the
/// prior one. Pass `None` for a fresh summary.
///
/// The single-pass token estimate is performed once at the top of this
/// function and reused for L1/L2/L3 budget checks. Each level's output
/// still gets its own `compacted_tokens` (a different message set) but
/// the *input* `original_tokens` is only computed once.
pub async fn apply_compaction(
    messages: &[Message],
    compact_range: std::ops::Range<usize>,
    token_budget: usize,
    provider: Option<&dyn CompactionProvider>,
    previous_summary: Option<&str>,
) -> Result<CompactionResult, ContextError> {
    let range_len = compact_range.end.saturating_sub(compact_range.start);
    if range_len == 0 || messages.is_empty() {
        return Ok(CompactionResult {
            compacted_indices: vec![],
            applied_level: 0,
            summary: SummaryMessage {
                role: "system".to_string(),
                summary: String::new(),
                message_count: 0,
            },
            original_tokens: 0,
            compacted_tokens: 0,
        });
    }

    let msgs_to_compact = &messages[compact_range.clone()];
    let original_tokens = estimate_tokens(msgs_to_compact);

    // Try Level 1: LLM summary (with previous-summary anchor when supplied)
    if let Some(p) = provider
        && let Ok(part) = compact_level1(
            msgs_to_compact,
            p,
            previous_summary,
            Some(original_tokens),
        )
        .await
        && part.compacted_tokens <= token_budget
    {
        return Ok(CompactionResult {
            compacted_indices: compact_range.collect(),
            applied_level: 1,
            summary: SummaryMessage {
                role: "assistant".to_string(),
                summary: part.content,
                message_count: msgs_to_compact.len(),
            },
            original_tokens,
            compacted_tokens: part.compacted_tokens,
        });
    }

    // Fall through to Level 2: structured truncation
    let l2_messages = compact_level2(msgs_to_compact);
    let l2_tokens = estimate_tokens(&l2_messages);
    if l2_tokens <= token_budget {
        return Ok(CompactionResult {
            compacted_indices: compact_range.collect(),
            applied_level: 2,
            summary: SummaryMessage {
                role: "assistant".to_string(),
                summary: messages_to_string(&l2_messages),
                message_count: msgs_to_compact.len(),
            },
            original_tokens,
            compacted_tokens: l2_tokens,
        });
    }

    // Fall through to Level 3: marker-only
    let l3_messages = compact_level3(msgs_to_compact);
    let l3_tokens = estimate_tokens(&l3_messages);
    Ok(CompactionResult {
        compacted_indices: compact_range.collect(),
        applied_level: 3,
        summary: SummaryMessage {
            role: "assistant".to_string(),
            summary: messages_to_string(&l3_messages),
            message_count: msgs_to_compact.len(),
        },
        original_tokens,
        compacted_tokens: l3_tokens,
    })
}

/// Fallback chain: tries Level 1 → Level 2 → Level 3 based on token budget.
///
/// Starts with LLM summarization (L1). If the result exceeds the budget,
/// falls back to structured truncation (L2). If that still exceeds the budget,
/// falls back to marker-only (L3).
///
/// `previous_summary` is forwarded to L1 to keep decision continuity across
/// successive compactions. Pass `None` to start a fresh anchor.
///
/// `precomputed_original_tokens`, when `Some(n)`, is forwarded to the
/// inner L1 call so the L4 path (`recovery_cascade::try_l4_compact`)
/// avoids a duplicate O(n) `estimate_tokens` scan over the same
/// message slice. When `None`, the inner L1 calls `estimate_tokens`
/// itself (existing behavior).
pub async fn compact_with_fallback(
    messages: &[Message],
    token_budget: usize,
    provider: Option<&dyn CompactionProvider>,
    previous_summary: Option<&str>,
    precomputed_original_tokens: Option<usize>,
) -> Vec<Message> {
    if messages.is_empty() {
        return Vec::new();
    }

    // Try Level 1 first
    if let Some(p) = provider {
        let result = compact_level1(
            messages,
            p,
            previous_summary,
            precomputed_original_tokens,
        )
        .await;
        if let Ok(part) = result
            && part.compacted_tokens <= token_budget
        {
            return vec![Message::assistant(&part.content)];
        }
        // compact_level1 already has internal fallback, but if we get an
        // error (shouldn't happen normally), try L2 below.
    }

    // Level 1 failed or exceeded budget → try Level 2
    let l2_messages = compact_level2(messages);
    let l2_tokens = estimate_tokens(&l2_messages);
    if l2_tokens <= token_budget {
        return l2_messages;
    }

    // Level 2 exceeded budget → try Level 3
    compact_level3(messages)
}

// ---- Local token helper ----

pub(crate) fn estimate_tokens(messages: &[Message]) -> usize {
    messages.iter().map(estimate_message_tokens).sum()
}
