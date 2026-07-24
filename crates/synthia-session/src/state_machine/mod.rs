//! Session state machine: validates state transitions and persists
//! them via [`Store`]. Splits into:
//!
//! * [`transitions`] — pure validation (`is_valid_transition`,
//!   `effect_for_entering`) and the `StateEnterEffect` /
//!   `StateMachineError` value types.
//! * [`machine`] — the stateful `SessionStateMachine` that owns one
//!   session's `current_state` and writes through to the store.

mod machine;
mod transitions;

pub use machine::SessionStateMachine;
pub use transitions::{
    StateEnterEffect,
    StateMachineError,
    effect_for_entering,
    is_valid_transition,
};
