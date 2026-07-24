use synthia_context::{
    compaction_service::{CompactionResult, compact_messages},
    token_budget::TokenBudget as ContextTokenBudget,
    traits::estimate_message_tokens,
};
use synthia_provider::types::Message;
use synthia_session::types::TokenBudget as SessionTokenBudget;

pub fn try_compact(
    messages: &mut Vec<Message>,
    budget: &SessionTokenBudget,
) -> Option<CompactionResult> {
    let context_budget = ContextTokenBudget::with_soft_limit(
        budget.hard_limit,
        budget.compaction_at,
    );
    let token_count: usize = messages.iter().map(estimate_message_tokens).sum();
    compact_messages(messages, &context_budget, token_count, 0.3)
}

pub fn try_compact_with_threshold(
    messages: &mut Vec<Message>,
    hard_limit: usize,
    soft_limit: usize,
) -> Option<CompactionResult> {
    let context_budget =
        ContextTokenBudget::with_soft_limit(hard_limit, soft_limit);
    let token_count: usize = messages.iter().map(estimate_message_tokens).sum();
    compact_messages(messages, &context_budget, token_count, 0.3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_try_compact_within_budget() {
        let mut messages =
            vec![Message::user("hello"), Message::assistant("hi")];
        let budget = SessionTokenBudget::new(100_000);
        let result = try_compact(&mut messages, &budget);
        assert!(result.is_none());
    }
}
