//! Canonical session model types — the state-machine record
//! (and the supporting state / config / token-budget types).
//!
//! This module is one half of the `Session` naming collision
//! documented in `crate::lib`. The struct in this file is the
//! state-machine model record (`types::Session`); the
//! `session::Session` legacy conversation record lives in the
//! `session` submodule. Consumers must always use the qualified
//! path (see `crate::lib` re-export policy).
//!
//! # Module Layout
//!
//! - [`state`]: [`state::SessionState`] enum (10 variants) +
//!   [`state::InvalidStateTransition`] error struct.
//! - [`config`]: [`config::SessionConfig`] struct + its
//!   `Default` impl (gpt-4o / 4096 tokens).
//! - `token_budget`: [`token_budget::TokenBudgetStatus`]
//!   enum + [`token_budget::TokenBudget`] struct + its
//!   4 methods + the 2 `CONTEXT_*` package-level constants.
//! - [`session`]: The [`session::Session`] struct + its 11
//!   methods (3 constructors + `assign_user` + 4 budget
//!   accessors + `record_token_usage` / `add_token_usage` +
//!   `transition_to` / `is_valid_transition`).
//! - [`tests`]: All 38 unit tests covering constructors,
//!   state-machine transitions, `TokenBudget` boundaries
//!   (8 dedicated + 3 dedicated `large_context` /
//!   `zero_tokens` / `with_thresholds_*`), context safety
//!   checks, and token-usage accumulation.

mod config;
mod session;
mod state;
mod token_budget;

#[cfg(test)]
mod tests;

pub use config::SessionConfig;
pub use session::Session;
pub use state::{InvalidStateTransition, SessionState};
pub use synthia_provider::types::TokenUsage;
pub use token_budget::{
    CONTEXT_HARD_MIN,
    CONTEXT_WARN_BELOW,
    TokenBudget,
    TokenBudgetStatus,
};
