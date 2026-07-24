//! Plugin loader for synthia-agent.
//!
//! [`AgentPluginLoader`] integrates the synthia-plugin
//! system with the agent lifecycle:
//!
//! - Initializes `PluginRegistry` on agent startup
//! - Discovers and loads user plugins from
//!   `~/.synthia/plugins/`
//! - Connects `HookRunner` events to agent lifecycle
//!   hooks (AgentStart / SessionStart / PreToolUse /
//!   PostToolUse)
//!
//! # Module Layout
//!
//! - [`error`]: [`error::PluginLoaderError`] enum (3
//!   variants: DiscoveryFailed / HookLoadFailed /
//!   HomeDirectoryNotFound).
//! - [`core`]: [`core::AgentPluginLoader`] struct +
//!   2 constructors (`new` / `with_plugins_dir`) + 5
//!   accessor methods (hook_runner / plugin_count /
//!   get_plugin / all_plugins / Debug impl) +
//!   [`core::user_plugins_path`] helper.
//! - [`discovery`]: 3 private discovery methods
//!   ([`discovery::AgentPluginLoader::discover_user_plugins`] +
//!   [`discovery::AgentPluginLoader::discover_plugins_from`] +
//!   [`discovery::AgentPluginLoader::load_plugin_hooks`]).
//! - [`fire`]: 4 hook-fire methods
//!   ([`fire::AgentPluginLoader::fire_agent_start`] +
//!   [`fire::AgentPluginLoader::fire_session_start`] +
//!   [`fire::AgentPluginLoader::fire_pre_tool_use`] +
//!   [`fire::AgentPluginLoader::fire_post_tool_use`]).
//! - [`tests`]: 7 unit tests covering empty dir, plugin
//!   loading, get_plugin, fire_events, hooks loading, and
//!   hook_runner access.

mod core;
mod discovery;
mod error;
mod fire;

#[cfg(test)]
mod tests;

pub use core::AgentPluginLoader;

pub use error::PluginLoaderError;
