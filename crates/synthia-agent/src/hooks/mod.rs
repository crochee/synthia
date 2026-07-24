//! Agent hook executor.
//!
//! Thin wrapper around [`synthia_hook::HookRegistry`] that adds
//! fail-open panic catching to every dispatch. If a hook panics (or
//! its async body returns an error), the executor logs a warning and
//! returns the safe default ([`ToolAction::Proceed`] for hooks that
//! return a verdict, `()` for fire-and-forget hooks).
//!
//! # Module Layout
//!
//! - [`executor`]: The [`executor::HookExecutor`] struct itself, plus
//!   `new` / `is_empty` / `Default` impls.
//! - [`lifecycle`]: The six `fire_*` methods covering the canonical
//!   agent lifecycle (`before_llm` / `before_tool` / `after_tool` /
//!   `after_llm` / `iteration_end` / `complete`). These are the
//!   methods the agent loop calls on every turn.
//! - [`domain`]: The three `on_*` methods covering domain-specific
//!   events that need a hook reaction but are not part of the
//!   canonical lifecycle (`on_tool_error` / `on_loop_detected` /
//!   `on_session_end`).
//! - [`catch_unwind`]: The private `catch_unwind` helper that turns
//!   a panic in the hook body into a `Result::Err(())` so the
//!   lifecycle / domain methods can fail-open.
//! - [`tests`]: All 12 unit tests, the three test-only hook fixtures
//!   (`PanickingHook`, `CountingHook`, `ModifyingHookForExecutor`),
//!   and the `make_context` test helper.

mod catch_unwind;
mod domain;
mod executor;
mod lifecycle;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use executor::HookExecutor;
