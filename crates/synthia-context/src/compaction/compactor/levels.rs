//! The four private per-level implementations on
//! [`super::core::Compactor`]:
//!
//! - [`level1_summary`]: Level 1 path *without* an LLM provider
//!   (always uses the structured-fallback path).
//! - [`level1_summary_with_provider`]: Level 1 path *with* an LLM
//!   provider; falls back to structured on None / failure / empty.
//! - [`level2_truncate`]: Level 2 structured truncation (delegates
//!   to [`super::super::level2::compact_level2`]).
//! - [`level3_marker_only`]: Level 3 marker-only retention
//!   (delegates to [`super::super::level3::compact_level3`]).
//!
//! These are deliberately not `pub`: callers must go through the
//! [`super::dispatch`] entry points, which compute `original_tokens`
//! and wrap the output in [`CompactionPart`].

use synthia_provider::Message;

use super::{
    super::{
        level1::CompactionProvider,
        level2::compact_level2,
        level3::compact_level3,
        util::{cap_to_head_tail, messages_to_string},
    },
    core::Compactor,
};
use crate::types::{CompactionPart, ContextError};

pub(super) fn level1_summary(
    compactor: &Compactor,
    messages: &[Message],
    original_tokens: usize,
) -> Result<CompactionPart, ContextError> {
    if messages.is_empty() {
        return Ok(CompactionPart {
            content: String::new(),
            original_tokens,
            compacted_tokens: 0,
        });
    }

    let summary = super::super::level1::build_structured_summary_fallback(
        messages,
        None,
        compactor.max_output_lines,
    );
    let compacted_tokens = Compactor::estimate_token_count(&summary);

    Ok(CompactionPart {
        content: summary,
        original_tokens,
        compacted_tokens,
    })
}

pub(super) async fn level1_summary_with_provider(
    compactor: &Compactor,
    messages: &[Message],
    original_tokens: usize,
    provider: Option<&dyn CompactionProvider>,
    previous_summary: Option<&str>,
) -> Result<CompactionPart, ContextError> {
    if messages.is_empty() {
        return Ok(CompactionPart {
            content: String::new(),
            original_tokens,
            compacted_tokens: 0,
        });
    }

    // Re-anchor via the level1 module's helper, but with
    // `compactor.max_output_lines` so the LLM-fail fallback path
    // matches the LLM-succeed path's output shape.
    let truncated_prev = previous_summary.map(|p| {
        use super::super::level1::{
            PREVIOUS_SUMMARY_HEAD_RATIO,
            PREVIOUS_SUMMARY_MAX_CHARS,
        };
        cap_to_head_tail(
            p,
            PREVIOUS_SUMMARY_MAX_CHARS,
            PREVIOUS_SUMMARY_HEAD_RATIO,
        )
    });

    if let Some(p) = provider {
        match p
            .generate_summary(messages, truncated_prev.as_deref())
            .await
        {
            Ok(summary) if !summary.is_empty() => {
                let compacted_tokens =
                    Compactor::estimate_token_count(&summary);
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
                // Provider failed, fall back to structured summary
            }
        }
    }

    // Fallback: build a structured summary at the configured
    // `max_output_lines`, prefixed with the (truncated) anchor.
    let summary = super::super::level1::build_structured_summary_fallback(
        messages,
        truncated_prev.as_deref(),
        compactor.max_output_lines,
    );
    let compacted_tokens = Compactor::estimate_token_count(&summary);

    Ok(CompactionPart {
        content: summary,
        original_tokens,
        compacted_tokens,
    })
}

pub(super) fn level2_truncate(
    _compactor: &Compactor,
    messages: &[Message],
    original_tokens: usize,
) -> Result<CompactionPart, ContextError> {
    if messages.is_empty() {
        return Ok(CompactionPart {
            content: String::new(),
            original_tokens,
            compacted_tokens: 0,
        });
    }

    let compacted = compact_level2(messages);
    let content = messages_to_string(&compacted);
    let compacted_tokens = Compactor::estimate_token_count(&content);

    Ok(CompactionPart {
        content,
        original_tokens,
        compacted_tokens,
    })
}

pub(super) fn level3_marker_only(
    _compactor: &Compactor,
    messages: &[Message],
    original_tokens: usize,
) -> Result<CompactionPart, ContextError> {
    if messages.is_empty() {
        return Ok(CompactionPart {
            content: String::new(),
            original_tokens,
            compacted_tokens: 0,
        });
    }

    let compacted = compact_level3(messages);
    let content = messages_to_string(&compacted);
    let compacted_tokens = Compactor::estimate_token_count(&content);

    Ok(CompactionPart {
        content,
        original_tokens,
        compacted_tokens,
    })
}
