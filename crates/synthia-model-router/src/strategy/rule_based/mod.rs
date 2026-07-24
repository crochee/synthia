//! Rule-based routing family.
//!
//! The original 768-line `rule_based.rs` was split
//! into focused submodules by responsibility:
//!
//! - [`types`]: the [`types::RoutingRule`] data
//!   carrier + its `new` / `with_triggers` /
//!   `with_active_turns` builders.
//! - [`strategy`]: the [`strategy::RuleBasedStrategy`]
//!   struct itself + its `new` constructor (which
//!   pre-sorts the rule list by descending priority).
//! - [`evaluate`]: the inherent `evaluate_trigger` /
//!   `evaluate_triggers` methods — the 60-line
//!   `match` block over [`crate::types::RoutingTrigger`]
//!   plus the conjunction check.
//! - [`routing_strategy`]: the `impl
//!   ` [`crate::types::RoutingStrategy`] for
//!   [`strategy::RuleBasedStrategy`] block — the
//!   async `route()` entry point.
//!
//! The 23 unit tests live in [`tests`].
//!
//! The 2 `RuleBasedStrategy` fields (`rules` /
//! `conversation_analyzer`) are `pub(super)` so
//! [`routing_strategy`] and [`evaluate`] can reach
//! them. External callers see only the public
//! surface preserved at the top of this file.

mod evaluate;
mod routing_strategy;
mod strategy;
mod types;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use strategy::RuleBasedStrategy;
pub use types::RoutingRule;
