//! Path resolvers for every config type the runtime reads.
//!
//! The 5 typed resolvers compute the canonical
//! `.agents/<file>` path under `workspace_root`. The main
//! [`resolve_config_path`] additionally checks
//! `~/.config/synthia/config.toml` as a fallback so
//! `ConfigWatcher::new` can succeed in environments where
//! the workspace config does not exist yet.
//!
//! [`resolve_all_config_paths`] fans out to all 5 typed
//! resolvers and returns a `HashMap<ConfigType, PathBuf>`
//! for callers that bootstrap the whole config surface
//! at once (e.g. `MultiConfigWatcher::watch_all`).

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use super::types::ConfigType;

pub fn resolve_config_path(workspace_root: &Path) -> PathBuf {
    let workspace_config = workspace_root.join(".agents").join("config.toml");
    if workspace_config.exists() {
        return workspace_config;
    }

    if let Ok(home) = std::env::var("HOME") {
        let home_path = PathBuf::from(home);
        let global_config = home_path
            .join(".config")
            .join("synthia")
            .join("config.toml");
        if global_config.exists() {
            return global_config;
        }
    }

    workspace_config
}

pub fn resolve_provider_config_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".agents").join("providers.toml")
}

pub fn resolve_skill_config_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".agents").join("skills")
}

pub fn resolve_permission_config_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".agents").join("permissions.toml")
}

pub fn resolve_mcp_config_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".agents").join("mcp.toml")
}

pub fn resolve_all_config_paths(
    workspace_root: &Path,
) -> HashMap<ConfigType, PathBuf> {
    let mut paths = HashMap::new();
    paths.insert(ConfigType::Main, resolve_config_path(workspace_root));
    paths.insert(
        ConfigType::Provider,
        resolve_provider_config_path(workspace_root),
    );
    paths.insert(ConfigType::Skill, resolve_skill_config_path(workspace_root));
    paths.insert(
        ConfigType::Permission,
        resolve_permission_config_path(workspace_root),
    );
    paths.insert(ConfigType::Mcp, resolve_mcp_config_path(workspace_root));
    paths
}
