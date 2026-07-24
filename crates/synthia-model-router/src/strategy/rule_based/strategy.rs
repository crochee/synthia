//! [`RuleBasedStrategy`] — the strategy struct itself.
//!
//! The strategy holds the rule list and a
//! [`ConversationAnalyzer`]. The actual dispatch
//! (turn-by-turn `route()` call, trigger evaluation)
//! lives in two separate submodules:
//!
//! - [`super::evaluate`]: the inherent `evaluate_trigger`
//!   / `evaluate_triggers` methods that test a single
//!   trigger (or a list of triggers) against the
//!   current `ConversationMetrics` and `&[Message]`
//!   window.
//! - [`super::routing_strategy`][]: the
//!   `impl RoutingStrategy for RuleBasedStrategy`
//!   block — the async entry point that picks the
//!   first matching rule and returns the chosen
//!   model.
//!
//! Keeping the struct + its constructor separate
//! from the async dispatch surface lets a reader
//! understand "what is the strategy" without
//! immediately diving into the 60-line `match` over
//! [`crate::types::RoutingTrigger`].

use super::types::RoutingRule;
use crate::analyzer::ConversationAnalyzer;

/// Rule-based routing strategy. Walks the
/// (pre-sorted-by-priority) rule list on every
/// `route()` call and returns the model targeted by
/// the first rule whose triggers all match.
pub struct RuleBasedStrategy {
    /// Pre-sorted-by-descending-priority rule list.
    /// Mutated only at construction time
    /// (see [`Self::new`]) — the `route()` path holds
    /// an immutable borrow over it.
    pub(super) rules: Vec<RoutingRule>,
    /// Single-shot analyzer the `route()` method
    /// calls to turn the current `&[Message]`
    /// window into a `ConversationMetrics`. Stored
    /// on the struct (not rebuilt per call) so the
    /// analyzer can carry forward tunables in the
    /// future; today it's just a default
    /// constructor.
    pub(super) conversation_analyzer: ConversationAnalyzer,
}

impl RuleBasedStrategy {
    /// Build a new strategy from an unsorted rule
    /// list. The list is **sorted in place** by
    /// descending `priority` so the `route()` loop
    /// doesn't have to re-rank on every call. Rules
    /// with equal priority retain their input order.
    pub fn new(rules: Vec<RoutingRule>) -> Self {
        let mut rules = rules;
        rules.sort_by_key(|r| -r.priority);
        Self {
            rules,
            conversation_analyzer: ConversationAnalyzer::new(),
        }
    }
}
