//! Pure transition-validation logic: which states can move to which, and what
//! side effect should fire when a state is entered.
//!
//! Kept dependency-free (no `Store`, no `Session`) so the validation rules
//! can be unit-tested in isolation and reused by other modules.

mod effect;
mod error;
mod validation;

#[cfg(test)]
mod tests;

pub use effect::{StateEnterEffect, effect_for_entering};
pub use error::StateMachineError;
pub use validation::is_valid_transition;
