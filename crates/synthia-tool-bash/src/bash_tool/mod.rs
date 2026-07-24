//! Bash shell execution tool with permission gating and
//! defense-in-depth command blacklisting.
//!
//! This tool implements the `synthia_tool::Tool` trait
//! so that, once registered in the `ToolRegistry`, every
//! invocation is routed through `PermissionChecker::check`
//! (see
//! `synthia-tool/src/registry/registration.rs::run_with_context`).
//! The `CommandBlacklist` field below provides an
//! additional, in-tool defense-in-depth layer: even if
//! the policy is mis-configured to allow a dangerous
//! command, the blacklist still rejects the well-known
//! destructive patterns. The two checks are intentionally
//! AND-logic: policy Allow ∧ blacklist Allow → execute;
//! anything else → `ToolOutput::error`.
//!
//! # Module Layout
//!
//! - [`builder`]: [`builder::BashTool`] struct + 2
//!   constructors (`new` / `with_default_timeout` etc.) +
//!   [`builder::command_manager`] accessor.
//! - [`executor`](executor): the
//!   [`executor::BashTool::execute_command`] low-level
//!   helper (returns `(stdout, stderr, exit_code,
//!   truncated)`).
//! - [`trait_impl`]: the [`Tool`] trait impl for
//!   [`BashTool`] — `name` / `description` / `parameters` /
//!   `requires_permission` / `is_concurrency_safe` /
//!   `call` (the main entry point that combines the
//!   blacklist check + foreground / background
//!   dispatch + output formatting).
//! - [`tests`]: 13 unit tests covering tool metadata,
//!   `execute_command` behavior, the blacklist gate, and
//!   the [`cap_to_char_boundary`] UTF-8 contract.

mod builder;
mod executor;
mod trait_impl;

#[cfg(test)]
mod tests;

pub use builder::BashTool;

/// Tool name used for registry lookups, `PermissionChecker` decisions,
/// event emission, and audit logs. Lower-case to match the rest of the
/// codebase (PBAC, recovery cascade, telemetry tests all key on
/// `"bash"`).
pub const TOOL_NAME: &str = "bash";

/// UTF-8 safe byte-boundary truncation. Canonical home is
/// `synthia_core::cap_to_char_boundary`; re-exported here so the
/// existing call sites (`execute_command` above) can keep using the
/// same name without touching every callsite. The single source of
/// truth lives in `crates/synthia-core/src/text.rs`.
pub use synthia_core::cap_to_char_boundary;

pub type Result<T> = std::result::Result<T, synthia_core::Error>;
