//! Test suite for `state/`.
//!
//! Split into focused submodules by concern, so the
//! production file stays a single, small container. Each
//! submodule tests exactly one of the surfaces the
//! server holds:
//!
//! - [`resolve_agent_name`] — the three-tier fallback
//!   ladder every dispatch flows through.
//! - [`build_prompt_context`] — the skills-directory
//!   walker and agent-registry snapshotter.
//! - [`for_test`] — `AppState::for_test`'s registry
//!   wiring pin.
//! - [`unapplied_keys`] — the R55 audit that classifies
//!   `agents.<name>` keys the boot ignores.
//! - [`resolve_server`] — R53: the `--config <path>`
//!   named-file reader.
//! - [`load_helpers`] — the R16 / R58 / R50
//!   `load_default_*` readers the run factory consumes.
//! - [`boot_surface`] — the optional integrations the
//!   boot wires (MCP).
//! - [`apply_tools`] — the R34 `[tools]` boot
//!   application.
//! - [`self_mcp`] — the self-management MCP server the
//!   boot publishes (`mcp__self__*`).
//! - [`support`] — the shared fixtures (`StubAgent`,
//!   `ScopedHome`, `empty_descriptor`, …) only.

mod apply_tools;
mod boot_surface;
mod build_prompt_context;
mod for_test;
mod load_helpers;
mod load_strategy;
mod resolve_agent_name;
mod resolve_server;
mod self_mcp;
mod support;
mod unapplied_keys;
