use synthia_provider::Message;

use super::{
    fallback::build_structured_summary_fallback,
    helpers::{estimate_token_count, estimate_tokens},
    provider::CompactionProvider,
};
use crate::types::{CompactionPart, ContextError};

/// Level 1: LLM summarization via provider call.
///
/// Sends messages to the LLM and asks for a structured summary.
/// Falls back to a heuristic summary if the provider fails or returns empty.
///
/// `previous_summary` is forwarded to the provider so the LLM can update
/// the prior anchor (decision continuity). The fallback path also embeds
/// the previous summary in its structured output.
///
/// `precomputed_original_tokens`, when `Some(n)`, is used as the
/// `original_tokens` value in the returned `CompactionPart` and skips the
/// internal `estimate_tokens(messages)` call. When `None`, the existing
/// behavior (call `estimate_tokens(messages)`) is preserved. The L4 path
/// (`recovery_cascade::try_l4_compact`) supplies `Some(n)` to avoid a
/// duplicate O(n) scan over the same message slice.
pub async fn compact_level1(
    messages: &[Message],
    provider: &dyn CompactionProvider,
    previous_summary: Option<&str>,
    precomputed_original_tokens: Option<usize>,
) -> Result<CompactionPart, ContextError> {
    if messages.is_empty() {
        return Ok(CompactionPart {
            content: String::new(),
            original_tokens: precomputed_original_tokens.unwrap_or(0),
            compacted_tokens: 0,
        });
    }

    let original_tokens = match precomputed_original_tokens {
        Some(n) => n,
        None => estimate_tokens(messages),
    };

    // Try LLM summarization via provider
    match provider.generate_summary(messages, previous_summary).await {
        Ok(summary) if !summary.is_empty() => {
            let compacted_tokens = estimate_token_count(&summary);
            return Ok(CompactionPart {
                content: summary,
                original_tokens,
                compacted_tokens,
            });
        }
        Ok(_) => {
            // Provider returned empty, fall through to fallback
        }
        Err(_) => {
            // Provider failed, fall through to fallback
        }
    }

    // Fallback: build a heuristic structured summary, prefixed with the
    // previous-summary anchor block when one is supplied.
    let summary =
        build_structured_summary_fallback(messages, previous_summary, 1);
    let compacted_tokens = estimate_token_count(&summary);

    Ok(CompactionPart {
        content: summary,
        original_tokens,
        compacted_tokens,
    })
}
