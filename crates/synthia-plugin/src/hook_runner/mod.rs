//! Hook runner for the plugin hook system.
//!
//! Provides [`core::HookRunner`] that loads hooks from `hooks.json`,
//! matches events with regex patterns, executes handlers in priority
//! order, and supports short-circuit behavior via
//! `HookResult::Stop`.
//!
//! # Module Layout
//!
//! - [`types`]: All simple data types — the deserialization helpers
//!   ([`types::RawHook`], [`types::RawHooks`]), the error enum
//!   ([`types::HookRunnerError`]), the per-call result type
//!   ([`types::SingleHookResult`]), the event-metadata
//!   ([`types::HookMetadata`]), and the runner-wide config
//!   ([`types::HookRunnerConfig`]).
//! - [`core`]: The [`core::HookRunner`] struct itself, plus
//!   construction ([`core::HookRunner::new`],
//!   [`core::HookRunner::with_base_dir`]), setters
//!   ([`core::HookRunner::with_fail_mode`],
//!   [`core::HookRunner::with_default_timeout`]),
//!   accessors ([`core::HookRunner::len`],
//!   [`core::HookRunner::is_empty`],
//!   [`core::HookRunner::configs`]),
//!   [`core::HookRunner::Default`], and the
//!   [`core::SharedHookRunner`] thread-safe wrapper type.
//! - [`load`]: The hooks.json loading pipeline:
//!   [`load::load_from_path`] → [`load::load_from_file`] →
//!   [`load::load_from_json`] → [`load::parse_raw_hooks`]. The
//!   loader tries the `{ "hooks": [...] }` envelope first, then
//!   falls back to a bare array. Hooks are sorted by priority
//!   (lower = first).
//! - [`fire`]: The [`fire::fire`] method — the public event
//!   dispatch. Iterates all loaded hooks in priority order, applies
//!   the regex matcher (target or extras), calls
//!   [`execute::execute_hook`], records the result, and applies
//!   short-circuit logic via `HookResult::Stop` /
//!   `HookResult::Failed` under `FailMode::Closed`.
//! - [`execute`]: The actual hook execution: [`execute::execute_hook`]
//!   dispatches on [`HookHandler`] (Command vs Prompt), and
//!   [`execute::execute_command`] parses the command into program
//!   + args (preventing shell injection), blocks known dangerous
//!     interpreters (sh, bash, rm, dd, mkfs, …), and runs with
//!     `tokio::time::timeout`.
//! - [`tests`]: All 9 unit tests covering hooks.json loading
//!   (array form, object form, invalid regex), fire behavior
//!   (match / no-match / wrong event / priority ordering), and the
//!   [`SharedHookRunner`] wrapper.

mod core;
mod execute;
mod fire;
mod load;
mod types;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use core::{HookRunner, SharedHookRunner};

pub use types::{HookMetadata, HookRunnerConfig};
