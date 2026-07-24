//! Config hot-reload support.
//!
//! Monitors the workspace config files for changes using the `notify` crate,
//! debounces rapid edits, validates new configs, and atomically swaps the
//! shared config reference.
//!
//! Supported config types:
//! - Provider configuration (providers.toml)
//! - Skill configuration (skills/ directory)
//! - Permission policy (permissions.toml)
//! - MCP server configuration (mcp.toml)
//! - Agent behavior parameters (max_iterations, token_budget, etc.)
//!
//! Submodule layout:
//!
//! - [`types`]: [`ConfigChangeCallback`] type alias,
//!   [`ConfigType`] enum + Display, [`SynthiaConfig`] struct +
//!   Default + `validate` + `load_from_file`,
//!   [`HotReloadableFields`] struct + `diff` +
//!   `changed_field_names` + `is_empty`, [`SharedConfig`]
//!   type alias.
//! - [`coordinator`]: private [`WatcherMessage`] enum + the
//!   private [`ReloadCoordinator`] that debounces reloads
//!   and atomically swaps the shared config. Also defines
//!   the `DEFAULT_DEBOUNCE_WINDOW` constant.
//! - [`watcher`]: the public [`ConfigWatcher`] — owns the
//!   `notify::RecommendedWatcher` and the debouncer task
//!   spawned in `new`.
//! - [`paths`]: the 6 free `resolve_*_path` helpers + the
//!   aggregator `resolve_all_config_paths`.
//! - [`multi`]: the public [`MultiConfigWatcher`] — manages
//!   one [`ConfigWatcher`] per [`ConfigType`] with shared
//!   callback fan-out.
//!
//! Unit tests live in [`tests`].

mod coordinator;
mod multi;
mod paths;
#[cfg(test)]
mod tests;
mod types;
mod watcher;

pub use multi::MultiConfigWatcher;
pub use paths::{
    resolve_all_config_paths,
    resolve_config_path,
    resolve_mcp_config_path,
    resolve_permission_config_path,
    resolve_provider_config_path,
    resolve_skill_config_path,
};
pub use types::{
    ConfigChangeCallback,
    ConfigType,
    HotReloadableFields,
    SharedConfig,
    SynthiaConfig,
};
pub use watcher::ConfigWatcher;
