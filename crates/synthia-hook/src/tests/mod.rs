//! Unit tests for the `synthia-hook` crate.
//!
//! Coverage map (13 tests):
//!
//! - [`basics`]: 5 tests
//!   ([`basics::TestHook`] mock + register / unregister / fire-in-order,
//!   [`basics::tool_action_variants`], [`basics::agent_context_creation`],
//!   [`basics::toolcall_from_value`], [`basics::toolcall_to_value_roundtrip`],
//!   [`basics::message_roundtrip`]).
//! - [`panic_recovery`]: 7 tests
//!   ([`panic_recovery::PanickingHook`] mock + 6 panicking-fire tests
//!   covering all hook points, [`panic_recovery::mixed_hooks_panicking_one_continues`],
//!   [`panic_recovery::multiple_hooks_second_panics_third_still_executes`],
//!   [`panic_recovery::catch_unwind_with_assert_unwind_safe`]).
//! - [`modify`]: 3 tests
//!   ([`modify::ModifyHookNameOnly`] / [`modify::ModifyHookInputOnly`] /
//!   [`modify::ModifyHookBoth`] mocks + 3 tests covering each
//!   `ToolAction::Modify` shape).

mod basics;
mod modify;
mod panic_recovery;
