//! Model router: maps a request (task type / routing context) to a
//! concrete provider/model pair, with primary → fallback → backup
//! chain resolution and cost-aware selection.
//!
//! Submodule layout:
//!
//! - [`config`]: `RoutingConfig`, `RoutingRule`, `RoutingCondition`,
//!   `TaskType`, `ComplexityLevel`, `RoutingContext`, TOML loader.
//! - [`evaluator`]: `RuleEvaluator` — pure rule evaluation and
//!   cost-budget filtering.
//! - [`model_router`]: the stateful `ModelRouter` struct (provider
//!   registration, selection methods, async availability checks) and
//!   the `FallbackChainConfig` / `TomlConfig` helpers used by
//!   `load_fallback_chain_from_toml`.
//!
//! Tests live in `tests.rs` to keep the public surface here minimal.

mod config;
mod evaluator;
mod model_router;

#[cfg(test)]
mod tests;

pub use config::*;
pub use evaluator::RuleEvaluator;
pub use model_router::{FallbackChainConfig, ModelRouter};
