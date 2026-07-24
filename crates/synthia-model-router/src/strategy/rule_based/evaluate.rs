//! Trigger evaluation — the per-trigger
//! `match`/`cmp` block that lives at the heart of the
//! rule engine.
//!
//! Two inherent methods on [`super::strategy::RuleBasedStrategy`]:
//!
//! - [`RuleBasedStrategy::evaluate_trigger`]: tests a
//!   single [`RoutingTrigger`] against the current
//!   `ConversationMetrics` and the `&[Message]`
//!   window. The 60-line `match` block.
//! - [`RuleBasedStrategy::evaluate_triggers`]:
//!   short-circuits "all triggers must match" — the
//!   conjunction check that gates a rule firing.
//!
//! Kept separate from [`super::routing_strategy`] so
//! the trigger semantics can be unit-tested directly
//! and so the async dispatch surface stays a tight
//! 20-line "walk the rule list" loop.

use synthia_provider::Message;

use super::strategy::RuleBasedStrategy;
use crate::types::{
    Comparison,
    ConversationMetrics,
    KeywordMatch,
    RoutingTrigger,
};

impl RuleBasedStrategy {
    /// Test a single [`RoutingTrigger`] against the
    /// current `ConversationMetrics` and the most
    /// recent user message in the conversation
    /// window.
    ///
    /// Most triggers are pure `metrics.*` checks
    /// (`Complexity`, `ConsecutiveTools`,
    /// `ConsecutiveFailures`, `FirstTurn`,
    /// `ToolFailure`); the two that need the
    /// conversation slice (`Keywords`,
    /// `MessageLength`) walk the messages in reverse
    /// to find the most recent user message and use
    /// its `extract_text()` content. If the slice is
    /// empty (or no user message is present) the
    /// content-dependent triggers return `false` —
    /// they never panic.
    pub(super) fn evaluate_trigger(
        &self,
        trigger: &RoutingTrigger,
        metrics: &ConversationMetrics,
        conversation: &[Message],
    ) -> bool {
        match trigger {
            RoutingTrigger::Keywords { words, match_type } => {
                let Some(msg) = conversation
                    .iter()
                    .rev()
                    .find(|m| matches!(m.role, synthia_provider::Role::User))
                else {
                    return false;
                };
                let content = msg
                    .content
                    .extract_text()
                    .unwrap_or_default()
                    .to_lowercase();
                match match_type {
                    KeywordMatch::Any => words
                        .iter()
                        .any(|w| content.contains(&w.to_lowercase())),
                    KeywordMatch::All => words
                        .iter()
                        .all(|w| content.contains(&w.to_lowercase())),
                }
            }
            RoutingTrigger::Complexity { level, comparison } => {
                match comparison {
                    Comparison::Gte => metrics.complexity >= *level,
                    Comparison::Lte => metrics.complexity <= *level,
                    Comparison::Eq => metrics.complexity == *level,
                }
            }
            RoutingTrigger::ConsecutiveTools { count, comparison } => {
                match comparison {
                    Comparison::Gte => metrics.tool_call_count >= *count,
                    Comparison::Lte => metrics.tool_call_count <= *count,
                    Comparison::Eq => metrics.tool_call_count == *count,
                }
            }
            RoutingTrigger::ConsecutiveFailures { count } => {
                metrics.consecutive_failures >= *count
            }
            RoutingTrigger::FirstTurn => metrics.message_count == 1,
            RoutingTrigger::MessageLength { min, max } => {
                let last_msg_len = conversation
                    .iter()
                    .rev()
                    .find(|m| matches!(m.role, synthia_provider::Role::User))
                    .and_then(|m| m.content.extract_text())
                    .map(|t| t.len())
                    .unwrap_or(0);

                min.is_none_or(|m| last_msg_len >= m)
                    && max.is_none_or(|m| last_msg_len <= m)
            }
            RoutingTrigger::ToolFailure => metrics.consecutive_failures > 0,
        }
    }

    /// Conjunction over a list of triggers — all must
    /// match for the rule to fire. Short-circuits on
    /// the first failure, so the order of triggers
    /// within a rule matters for the cheap/fast ones
    /// first optimisation.
    pub(super) fn evaluate_triggers(
        &self,
        triggers: &[RoutingTrigger],
        metrics: &ConversationMetrics,
        conversation: &[Message],
    ) -> bool {
        triggers.iter().all(|trigger| {
            self.evaluate_trigger(trigger, metrics, conversation)
        })
    }
}
