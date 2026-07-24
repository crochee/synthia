//! [`RoutingRule`] — a single priority-sorted rule
//! that maps a set of [`RoutingTrigger`]s to a target
//! model + provider.
//!
//! The rule is the **unit of policy** in the rule-based
//! routing family: callers build a `Vec<RoutingRule>`
//! via [`RoutingRule::new`], chain [`with_triggers`]
//! and [`with_active_turns`] to populate the rest of
//! the fields, and hand the list to
//! [`super::strategy::RuleBasedStrategy::new`]. The
//! strategy then sorts by descending `priority` so the
//! highest-priority matching rule wins.
//!
//! `RoutingRule` is intentionally tiny — no
//! validation beyond what the `RoutingTrigger` enum
//! already enforces. Invalid configurations surface
//! as `Err` from the [`super::routing_strategy`]
//! `route()` call (e.g. "target model not found") or
//! simply as "no rules matched" if the triggers are
//! unsatisfiable.
//!
//! [`with_triggers`]: RoutingRule::with_triggers
//! [`with_active_turns`]: RoutingRule::with_active_turns

use crate::types::{ProviderType, RoutingTrigger};

/// One priority-sorted routing rule.
#[derive(Debug, Clone)]
pub struct RoutingRule {
    /// Human-readable name — used in
    /// [`crate::types::RoutingDecision::matched_rules`]
    /// when this rule fires, and surfaced in tracing
    /// events.
    pub name: String,
    /// Triggers that must ALL match for this rule to
    /// fire. Empty means "always matches" — a
    /// degenerate but legal configuration.
    pub triggers: Vec<RoutingTrigger>,
    /// Model name to route to when the rule fires.
    /// Must be one of the names in the
    /// `models: &[ModelConfig]` slice passed to
    /// [`super::routing_strategy::route`]; the
    /// dispatcher falls back to `models.first()` if
    /// the named model isn't present, then errors
    /// out only when the fallback also fails.
    pub target_model: String,
    /// Provider tag for the target model. Stored on
    /// the rule for introspection / observability —
    /// the dispatcher itself looks the model up by
    /// `name` in the available `models` list, not by
    /// provider.
    pub target_provider: ProviderType,
    /// Higher `priority` wins ties; the strategy
    /// sorts the rules list by descending priority
    /// in its constructor.
    pub priority: i32,
    /// Number of turns this rule stays active after
    /// it fires. **Currently not consulted by the
    /// dispatcher** — kept on the struct so config
    /// files that pin an active-TTL survive a code
    /// reload even though the engine doesn't yet
    /// honour it.
    pub active_turns: usize,
}

impl RoutingRule {
    /// Build a rule with no triggers and the default
    /// `active_turns = 5`. Callers typically follow
    /// up with [`with_triggers`](Self::with_triggers)
    /// and optionally
    /// [`with_active_turns`](Self::with_active_turns).
    pub fn new(
        name: &str,
        target_model: &str,
        target_provider: ProviderType,
        priority: i32,
    ) -> Self {
        Self {
            name: name.to_string(),
            triggers: Vec::new(),
            target_model: target_model.to_string(),
            target_provider,
            priority,
            active_turns: 5,
        }
    }

    /// Replace the trigger list. Returns the rule so
    /// it can chain.
    pub fn with_triggers(mut self, triggers: Vec<RoutingTrigger>) -> Self {
        self.triggers = triggers;
        self
    }

    /// Override the default `active_turns = 5`.
    pub fn with_active_turns(mut self, turns: usize) -> Self {
        self.active_turns = turns;
        self
    }
}
