use synthia_provider::{Message, Role};

use super::helpers::first_n_lines;
use crate::{
    compaction::util::{cap_to_head_tail, truncate_to_chars},
    traits::{extract_message_text, extract_message_tool_uses},
};

/// Maximum character budget for a `previous_summary` anchor before it gets
/// wrapped into a `<previous-summary>...</previous-summary>` block.
///
/// 4000 chars ≈ 1000 tokens (4 chars/token) which keeps the anchor block
/// from growing linearly across successive L1 compactions. See design
/// decision D3 in `cleanup-compact-fallback-prev-summary/design.md`.
pub const PREVIOUS_SUMMARY_MAX_CHARS: usize = 4000;

/// Head ratio (60%) used when capping a `previous_summary` anchor.
pub const PREVIOUS_SUMMARY_HEAD_RATIO: f64 = 0.6;

/// Heuristic structured-summary fallback for L1. Identical shape to
/// the LLM-prompted summary (sections = "Decisions / Tools Used /
/// Findings" + a `<previous-summary>` anchor block) so the output
/// looks the same whether or not the provider was available.
///
/// `max_output_lines` is the number of assistant-message text lines
/// to keep per response (the `Compactor::level1_summary_with_provider`
/// path passes `self.max_output_lines`; the bare `compact_level1`
/// path passes `1`).
pub(crate) fn build_structured_summary_fallback(
    messages: &[Message],
    previous_summary: Option<&str>,
    max_output_lines: usize,
) -> String {
    let mut decisions = Vec::new();
    let mut tools_used = Vec::new();
    let mut findings = Vec::new();

    let mut i = 0;
    while i < messages.len() {
        if matches!(messages[i].role, Role::User) {
            let text = extract_message_text(&messages[i]);
            if !text.is_empty() {
                findings.push(format!(
                    "- User request: {}",
                    truncate_to_chars(&text, 100)
                ));
            }
            if i + 1 < messages.len()
                && matches!(messages[i + 1].role, Role::Assistant)
            {
                let assistant_text = extract_message_text(&messages[i + 1]);
                let assistant_line =
                    first_n_lines(&assistant_text, max_output_lines);
                if !assistant_line.is_empty() {
                    decisions.push(format!(
                        "- Response: {}",
                        truncate_to_chars(&assistant_line, 100)
                    ));
                }
                i += 2;
            } else {
                i += 1;
            }
        } else if matches!(messages[i].role, Role::Assistant) {
            let tool_uses = extract_message_tool_uses(&messages[i]);
            for tu in &tool_uses {
                tools_used.push(format!(
                    "- {}({})",
                    tu.name,
                    truncate_to_chars(&tu.input.to_string(), 60)
                ));
            }
            let text = extract_message_text(&messages[i]);
            if !text.is_empty() && tool_uses.is_empty() {
                findings.push(format!(
                    "- Assistant: {}",
                    truncate_to_chars(&text, 100)
                ));
            }
            i += 1;
        } else {
            i += 1;
        }
    }

    let mut sections = Vec::new();
    sections.push(format!("[Summary of {} messages]", messages.len()));

    if !decisions.is_empty() {
        sections.push(format!("Decisions: {}", decisions.join(" ")));
    }
    if !tools_used.is_empty() {
        sections.push(format!("Tools Used: {}", tools_used.join(" ")));
    }
    if !findings.is_empty() {
        sections.push(format!("Findings: {}", findings.join(" ")));
    }

    if sections.len() == 1 {
        sections.push("[No significant content]".to_string());
    }

    // Anchor: prepend previous summary as a `<previous-summary>` block
    // when one is supplied, mirroring `build_structured_summary` so the
    // fallback path keeps decision continuity across successive L1
    // compactions.
    //
    // FU.5: cap the anchor at PREVIOUS_SUMMARY_MAX_CHARS so the block
    // doesn't grow linearly across successive L1 compactions.
    let summary_body = sections.join(" | ");
    match previous_summary {
        Some(prev) if !prev.is_empty() => {
            let truncated = cap_to_head_tail(
                prev,
                PREVIOUS_SUMMARY_MAX_CHARS,
                PREVIOUS_SUMMARY_HEAD_RATIO,
            );
            format!(
                "<previous-summary>\n{}\n</previous-summary>\n{}",
                truncated, summary_body
            )
        }
        _ => summary_body,
    }
}
