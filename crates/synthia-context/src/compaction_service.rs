use synthia_provider::Message;

use crate::{
    compactor::Compactor,
    token_budget::TokenBudget,
    traits::estimate_message_tokens,
};

pub struct CompactionResult {
    pub old_tokens: usize,
    pub new_tokens: usize,
    /// Compaction strategy identifier (e.g., `stage2-hard-clear`).
    pub implementation: String,
    /// Sub-step of the strategy (e.g., `replace`).
    pub phase: String,
    /// Number of messages that were compacted into the summary.
    pub messages_compacted: usize,
}

pub fn compact_messages(
    messages: &mut Vec<Message>,
    budget: &TokenBudget,
    token_count: usize,
    protection_ratio: f64,
) -> Option<CompactionResult> {
    if token_count <= budget.soft_limit {
        return None;
    }

    let level =
        Compactor::new(0).auto_select_level(token_count, budget.soft_limit);
    if level == 0 {
        return None;
    }

    if messages.len() < 2 {
        return None;
    }

    let split_point =
        (messages.len() as f64 * (1.0 - protection_ratio)) as usize;
    if split_point == 0 {
        return None;
    }

    let old_tokens: usize = messages.iter().map(estimate_message_tokens).sum();

    let to_compact = messages[..split_point].to_vec();
    let compactor = Compactor::new(level);

    // OTel `compaction` span. Created at the compaction trigger entry,
    // after all early-return guards have passed (so the span only fires
    // when compaction actually proceeds). `after_tokens` and
    // `messages_after` are declared as `tracing::field::Empty` at the
    // callsite and recorded after compaction completes — undeclared
    // fields are silent no-ops in `Span::record` (Task 7 lesson).
    //
    // Without the `otel` feature the `span!` macro, the guard, and the
    // post-compaction recording block are all compile-time eliminated;
    // `compact_messages` runs with zero span overhead.
    #[cfg(feature = "otel")]
    let messages_before = messages.len();
    #[cfg(feature = "otel")]
    let stage_name: &'static str = match level {
        1 => "L1",
        2 => "L2",
        3 => "L3",
        _ => "unknown",
    };
    #[cfg(feature = "otel")]
    let compaction_span = tracing::span!(
        target: "synthia.compaction",
        tracing::Level::INFO,
        "compaction",
        compaction.before_tokens = old_tokens,
        compaction.messages_before = messages_before,
        compaction.stage = %stage_name,
        compaction.after_tokens = tracing::field::Empty,
        compaction.messages_after = tracing::field::Empty,
    );
    #[cfg(feature = "otel")]
    let _compaction_guard = compaction_span.enter();

    let outcome = match compactor.compact(&to_compact) {
        Ok(part) => {
            let summary = Message::assistant(&part.content);
            let remaining = messages[split_point..].to_vec();
            messages.clear();
            messages.push(summary);
            messages.extend(remaining);
            let new_tokens: usize =
                messages.iter().map(estimate_message_tokens).sum();
            let (implementation, phase) = implementation_and_phase(level as u8);
            Some(CompactionResult {
                old_tokens,
                new_tokens,
                implementation,
                phase,
                messages_compacted: split_point,
            })
        }
        Err(_) => {
            let keep_from = messages.len() / 2;
            let remaining = messages[keep_from..].to_vec();
            messages.clear();
            messages.extend(remaining);
            None
        }
    };

    // Record post-compaction attributes on both success and error
    // paths — `messages` is mutated in either branch, so the after
    // state is always meaningful.
    #[cfg(feature = "otel")]
    {
        let after_tokens: usize =
            messages.iter().map(estimate_message_tokens).sum();
        compaction_span.record("compaction.after_tokens", after_tokens);
        compaction_span.record("compaction.messages_after", messages.len());
    }

    outcome
}

/// Map a compaction level to the canonical implementation / phase
/// strings used by [`CompactionAnalyticsAttempt`].
fn implementation_and_phase(level: u8) -> (String, String) {
    match level {
        1 => ("anchored-summary".to_string(), "compress".to_string()),
        2 => ("stage2-hard-clear".to_string(), "head-tail".to_string()),
        3 => ("stage3-pruning".to_string(), "replace".to_string()),
        _ => ("unknown".to_string(), "unknown".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token_budget::TokenBudget;

    #[test]
    fn test_compact_messages_within_budget() {
        let mut messages =
            vec![Message::user("hello"), Message::assistant("world")];
        let budget = TokenBudget::new(100_000);
        let token_count = 50;
        let result = compact_messages(&mut messages, &budget, token_count, 0.3);
        assert!(result.is_none());
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_compact_messages_exceeds_budget() {
        let mut messages = Vec::new();
        for i in 0..10 {
            messages.push(Message::user(format!(
                "user message {} with some content to add tokens",
                i
            )));
            messages.push(Message::assistant(format!(
                "assistant response {} with some content to add tokens",
                i
            )));
        }
        let budget = TokenBudget::new(10);
        let token_count = 500;
        let result = compact_messages(&mut messages, &budget, token_count, 0.3);
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.old_tokens > r.new_tokens);
        assert!(messages.len() < 20);
        assert!(!r.implementation.is_empty());
        assert!(!r.phase.is_empty());
        assert!(r.messages_compacted > 0);
    }

    #[test]
    fn test_compact_messages_too_few_messages() {
        let mut messages = vec![Message::user(
            "a very long message that exceeds the token budget by a lot but there is only one message so compaction cannot proceed",
        )];
        let budget = TokenBudget::new(10);
        let token_count = 100;
        let result = compact_messages(&mut messages, &budget, token_count, 0.3);
        assert!(result.is_none());
    }
}
