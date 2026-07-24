//! MCP server registry — the in-memory store of registered
//! [`McpServerInfo`] records + their cached schemas +
//! their optional [`crate::manager::McpManager`] connection
//! to live MCP servers.
//!
//! # Module Layout
//!
//! - [`types`]: [`types::McpFilter`] +
//!   [`types::McpServerInfo`] + its `RegistryItem` impl +
//!   its `From<&McpServerConfig>` impl.
//! - [`registry`]: [`registry::McpRegistry`] struct +
//!   `Default` + `Clone` + the 18 main methods (2 ctors +
//!   `add_config` / `discover_tools` / `get_tool_metadata`
//!   / `discover_all_tools` / `register_tools_to_registry`
//!   / `get_tool_schema` / `cache_tool_schema` /
//!   `clear_schema_cache` / `get_config` / `remove_config`
//!   / `list_configs` / `filter_servers` / `contains` /
//!   `len` / `is_empty`).
//! - [`registry_trait`]: The `Registry<McpServerInfo>`
//!   trait impl (4 methods: `register` / `unregister` /
//!   `get` / `list`).
//! - `lifecycle_trait`: The
//!   `LifecycleRegistry<McpServerInfo>` trait impl
//!   (3 methods: `start` / `stop` / `stop_all`).
//! - [`tests`]: 6 unit tests covering register/get,
//!   unregister, list+filter (transport / enabled_only),
//!   already-exists, contains/len, From<&McpServerConfig>.

mod lifecycle_trait;
#[allow(clippy::module_inception)]
mod registry;
mod registry_trait;
mod types;

#[cfg(test)]
mod tests;

pub use registry::McpRegistry;
pub use types::{McpFilter, McpServerInfo};
