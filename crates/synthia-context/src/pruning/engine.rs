//! The `prune` orchestrator: walks a `&mut [Message]` in reverse and
//! marks older tool-result messages for clearing once the protected
//! tail budget is exhausted. The actual `content` is **not** mutated
//! here — the renderer in `truncate.rs` is responsible for emitting
//! the placeholder body when it sees `tool_result_cleared_at = Some(_)`.
//!
//! The scan is **idempotent** and terminates early on the first
//! already-cleared message it encounters, so repeated calls are O(k)
//! where k is the size of the un-cleared prefix.
//!
//! The function is part of the
//! `prune-idempotent-marker` invariant family: a single
//! `tool_result_cleared_at: Option<DateTime<Utc>>` field on each
//! `Message` records the monotonic time of the prune pass that
//! touched it, replacing the old "mutate the content" approach.

use chrono::Utc;
use synthia_provider::Message;

use super::classify::is_tool_result;

/// Default token budget for the protected tail (last K tool-result
/// tokens kept verbatim). Aligned with OpenCode's `PRUNE_PROTECT`
/// constant.
pub const PRUNE_PROTECT_TOKENS: u32 = 40_000;

/// Statistics from a single `prune()` pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PruneStats {
    /// Number of tool-result messages newly marked as cleared.
    pub marked_count: usize,
    /// Cumulative tokens of tool-result messages kept in the protected tail.
    pub kept_tokens: u32,
    /// Total number of messages visited during the reverse scan.
    pub scanned_count: usize,
}

/// Idempotent single-pass tail protection.
///
/// Walks `messages` in reverse order, accumulating the estimated
/// tokens of recent tool-result messages. Once the cumulative token
/// count would exceed `protect_tokens`, older tool-result messages
/// are marked (`tool_result_cleared_at = Some(now)`) but their
/// `content` is left untouched. The scan terminates early when it
/// encounters a message that has already been marked, making the
/// function safe to call repeatedly.
///
/// A "tool-result message" is detected via [`is_tool_result`], which
/// recognizes the two existing on-the-wire shapes:
/// - `Role::User` with a `ContentPart::ToolResult` content (Anthropic
///   / OpenAI convention), and
/// - `Role::Tool` with a `ContentPart::ToolResult` content (legacy
///   shape).
///
/// Plain user-text / assistant-text / system messages are skipped —
/// they never count against the `PRUNE_PROTECT` budget and never get
/// marked.
///
/// # Returns
///
/// A [`PruneStats`] describing how many messages were marked, how
/// many tokens are kept in the protected tail, and how many messages
/// were scanned in total.
pub fn prune(messages: &mut [Message], protect_tokens: u32) -> PruneStats {
    let _span = tracing::info_span!(
        target: "synthia.pruning",
        "prune",
        protect_tokens = protect_tokens,
    )
    .entered();

    let mut stats = PruneStats::default();
    let mut kept_tokens: u32 = 0;
    let protect = protect_tokens;

    for msg in messages.iter_mut().rev() {
        stats.scanned_count += 1;

        // Idempotent stop: a previously cleared message is the natural
        // boundary of an earlier pass; everything older was already
        // marked and must remain so.
        if msg.tool_result_cleared_at.is_some() {
            break;
        }

        // Only tool-result messages count against the protected budget.
        if !is_tool_result(msg) {
            continue;
        }

        let tokens = crate::estimator::estimate_message_tokens(msg) as u32;
        if kept_tokens.saturating_add(tokens) > protect {
            msg.tool_result_cleared_at = Some(Utc::now());
            stats.marked_count += 1;
        } else {
            kept_tokens = kept_tokens.saturating_add(tokens);
        }
    }

    stats.kept_tokens = kept_tokens;

    tracing::info!(
        target: "synthia.pruning",
        marked_count = stats.marked_count,
        kept_tokens = stats.kept_tokens,
        scanned_count = stats.scanned_count,
        "prune completed"
    );

    // OTel counters (feature-gated). When the `otel` cargo feature is
    // disabled this block is compile-time eliminated — no `opentelemetry`
    // dependency, no counter overhead. When enabled, the global meter
    // provider (set up by `synthia-telemetry` when its `otel` feature is
    // active) exports the counters; if no provider is installed the
    // global meter is a no-op.
    #[cfg(feature = "otel")]
    {
        use opentelemetry::global;
        let meter = global::meter("synthia");
        let marked_counter =
            meter.u64_counter("synthia.pruning.marked_count").build();
        marked_counter.add(stats.marked_count as u64, &[]);
        let kept_counter =
            meter.u64_counter("synthia.pruning.kept_tokens").build();
        kept_counter.add(stats.kept_tokens as u64, &[]);
        let scanned_counter =
            meter.u64_counter("synthia.pruning.scanned_count").build();
        scanned_counter.add(stats.scanned_count as u64, &[]);
    }

    stats
}

#[cfg(test)]
mod tests {
    use synthia_provider::{Content, ContentPart, Message, Role, ToolResult};

    use super::*;

    fn tool_result_msg(id: &str, body: &str) -> Message {
        Message::new(
            Role::User,
            Content::Single(ContentPart::ToolResult(ToolResult {
                tool_use_id: id.to_string(),
                content: vec![ContentPart::Text(
                    synthia_provider::TextContent {
                        text: body.to_string(),
                        cache_control: None,
                    },
                )],
                structured_content: None,
                is_error: None,
            })),
        )
    }

    /// ~20K tokens: 20_000 * 4 chars = 80_000 chars.
    fn large_tool_result_text(tokens: usize) -> String {
        "x".repeat(tokens * 4)
    }

    #[test]
    fn test_prune_empty() {
        let mut msgs: Vec<Message> = vec![];
        let stats = prune(&mut msgs, 1000);
        assert_eq!(stats.marked_count, 0);
        assert_eq!(stats.kept_tokens, 0);
    }

    #[test]
    fn test_prune_no_tool_results() {
        let mut msgs = vec![
            Message::new(Role::User, Content::text("hello")),
            Message::new(Role::Assistant, Content::text("hi")),
        ];
        let stats = prune(&mut msgs, 1000);
        assert_eq!(stats.marked_count, 0);
        // Plain text doesn't count against the budget.
        assert_eq!(stats.kept_tokens, 0);
    }

    #[test]
    fn test_prune_marks_old_tool_results() {
        // Five huge tool-results (~20K tokens each) with a small
        // recent one; the older ones should all be marked once the
        // 40K-token budget is exhausted.
        let huge = large_tool_result_text(20_000);
        let mut msgs: Vec<Message> = (0..5)
            .map(|i| tool_result_msg(&format!("id-{i}"), &huge))
            .collect();
        msgs.push(tool_result_msg("recent", "small recent result"));
        let stats = prune(&mut msgs, PRUNE_PROTECT_TOKENS);
        assert!(stats.marked_count >= 1);
        // The recent small one is the only message guaranteed to
        // remain unmarked.
        assert!(msgs.last().unwrap().tool_result_cleared_at.is_none());
    }

    #[test]
    fn test_prune_idempotent() {
        let huge = large_tool_result_text(20_000);
        let mut msgs: Vec<Message> = (0..4)
            .map(|i| tool_result_msg(&format!("id-{i}"), &huge))
            .collect();
        let stats1 = prune(&mut msgs, PRUNE_PROTECT_TOKENS);
        let stats2 = prune(&mut msgs, PRUNE_PROTECT_TOKENS);
        // First pass marks at least one message.
        assert!(stats1.marked_count >= 1);
        // Second pass: the first cleared message is the stop signal,
        // so no new marks happen.
        assert_eq!(stats2.marked_count, 0);
    }

    #[test]
    fn test_prune_zero_budget_marks_all() {
        let mut msgs = vec![
            tool_result_msg("a", "aaaa"),
            tool_result_msg("b", "bbbb"),
            tool_result_msg("c", "cccc"),
        ];
        let stats = prune(&mut msgs, 0);
        // With a 0 budget every tool result overflows.
        assert_eq!(stats.marked_count, 3);
    }

    #[test]
    fn test_prune_never_marks_non_tool_messages() {
        let mut msgs = vec![
            Message::new(Role::User, Content::text("user text")),
            Message::new(Role::Assistant, Content::text("assistant text")),
            Message::new(Role::System, Content::text("system text")),
        ];
        let stats = prune(&mut msgs, 0);
        assert_eq!(stats.marked_count, 0);
    }

    #[test]
    fn test_prune_protect_tokens_default_constant() {
        assert_eq!(PRUNE_PROTECT_TOKENS, 40_000);
    }

    #[test]
    fn test_prune_emits_tracing_log_with_stats() {
        // Just verify prune does not panic with tracing enabled.
        // The tracing::info! call happens inside prune; full mock subscriber
        // testing would require tracing-test crate (Phase 2).
        let huge = large_tool_result_text(20_000);
        let mut msgs: Vec<Message> = (0..5)
            .map(|i| tool_result_msg(&format!("id-{i}"), &huge))
            .collect();

        let stats = prune(&mut msgs, PRUNE_PROTECT_TOKENS);
        assert!(stats.marked_count >= 1);
        assert!(stats.scanned_count > 0);
    }
}
