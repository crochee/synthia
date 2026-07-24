//! MCP manager — central lifecycle owner of every MCP server
//! connection.
//!
//! Submodule layout:
//!
//! - [`types`]: the [`ServerConnection`] and [`McpManager`] structs
//!   (with `pub(super)` fields) plus the rmcp + tokio type imports
//!   shared across the impl submodules.
//! - [`construct`]: the [`McpManager`] constructors
//!   (`new`, `with_idle_config`, `with_credential_store`,
//!   `with_hybrid_mode`, `with_hybrid_mode_and_idle_timeout`) and the
//!   [`Default`] impl. All five delegate to a single private
//!   `new_with_settings` helper that owns the field-init shape.
//! - [`lifecycle`]: connection lifecycle — `start`, `stop`, `restart`,
//!   `stop_all`, `get_status`.
//! - [`config`]: config registration + tool discovery
//!   (`register_config`, `discover_tools`,
//!   `discover_tools_for_server`, `discover_tools_fast`,
//!   `discover_tools_internal`, `get_discovery`).
//! - [`idle`]: idle tracking + recycling + the non-hybrid idle monitor
//!   (`record_activity`, `is_idle`, `get_idle_servers`,
//!   `recycle_idle_servers`, `start_idle_monitor`).
//! - [`tool_call`]: tool calling + the running-service accessor
//!   (`call_tool`, `is_connected`, `with_running_service_mut`,
//!   `credential_store`).
//! - [`hybrid`]: hybrid mode — lazy-connection tool calls,
//!   per-server hybrid state introspection, the discovered-tools
//!   cache, the runtime hybrid-mode toggle, and the hybrid idle
//!   cleanup background task.
//!
//! Unit tests live in [`tests`]; integration tests that also exercise
//! `McpRegistry` and `McpToolAdapter` live in [`integration_tests`].

mod config;
mod construct;
mod hybrid;
mod idle;
mod lifecycle;
mod tool_call;
mod types;

#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod tests;

pub use types::{McpManager, ServerConnection};
