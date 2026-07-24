//! Plugin registry: discovery, lifecycle, and handle management.
//!
//! This module owns the data model for *what a plugin is* (the
//! [`types::PluginPath`], [`types::HookConfig`],
//! [`types::McpServerConfig`] data structs) and the runtime handle
//! the agent loop uses to talk to a loaded plugin
//! ([`handle::PluginHandle`], [`handle::PluginRegistry`]).
//!
//! # Module Layout
//!
//! - [`types`]: The four data structs/types ([`types::PluginId`],
//!   [`types::PluginPath`], [`types::HookConfig`],
//!   [`types::McpServerConfig`]) plus their small impls
//!   ([`types::PluginPath::new`] / [`types::PluginPath::manifest_path`]
//!   / [`types::McpServerConfig::transport`]
//!   / [`types::McpServerConfig::validate`]).
//! - [`handle`]: The [`handle::PluginHandle`] struct plus
//!   [`handle::PluginHandle::load`],
//!   [`handle::PluginHandle::load_hooks`], and
//!   [`handle::PluginHandle::load_mcp_config`]. Each `load_*` method
//!   reads one config file (manifest, hooks, mcp) from the plugin
//!   directory and parses it.
//! - [`store`]: The [`store::PluginRegistry`] struct plus
//!   discovery ([`store::PluginRegistry::discover_user_plugins`],
//!   [`store::PluginRegistry::discover_project_plugins`]),
//!   lifecycle ([`store::PluginRegistry::load_plugin`],
//!   [`store::PluginRegistry::unload_plugin`]), and accessors
//!   ([`store::PluginRegistry::get`],
//!   [`store::PluginRegistry::get_by_name`],
//!   [`store::PluginRegistry::all`],
//!   [`store::PluginRegistry::len`],
//!   [`store::PluginRegistry::is_empty`],
//!   [`store::PluginRegistry::clear`]).
//! - [`tests`]: All 13 unit tests covering discovery, load/unload,
//!   duplicate detection, hooks and MCP config loading, error
//!   cases, and accessors.

mod handle;
mod store;
mod types;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use handle::PluginHandle;
pub use store::PluginRegistry;
pub use types::{HookConfig, McpServerConfig, PluginId, PluginPath};
