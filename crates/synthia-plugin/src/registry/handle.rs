//! The [`PluginHandle`] struct plus its three load methods
//! ([`PluginHandle::load`], [`PluginHandle::load_hooks`],
//!  [`PluginHandle::load_mcp_config`]).
//!
//! `PluginHandle` is the runtime view of a loaded plugin: a stable
//! [`PluginId`], the original [`PluginPath`], the parsed
//! [`PluginManifest`](crate::manifest::PluginManifest), and the
//! loaded hooks / MCP server configs. It is what the rest of the
//! agent loop talks to; the [`PluginRegistry`](super::registry::PluginRegistry)
//! is just a `HashMap<PluginId, PluginHandle>`.

use std::fs;

use uuid::Uuid;

use super::types::{HookConfig, McpServerConfig, PluginId, PluginPath};
use crate::{PluginError, manifest::PluginManifest};

/// A loaded plugin handle containing all plugin information
#[derive(Debug, Clone)]
pub struct PluginHandle {
    /// Unique identifier for this plugin
    pub id: PluginId,
    /// Path to the plugin directory
    pub path: PluginPath,
    /// Parsed plugin manifest
    pub manifest: PluginManifest,
    /// Hook configurations
    pub hooks: Vec<HookConfig>,
    /// MCP server configurations
    pub mcp_servers: Vec<McpServerConfig>,
}

impl PluginHandle {
    /// Load a plugin from a directory path
    pub fn load(path: &PluginPath) -> Result<Self, PluginError> {
        let manifest_path = path.manifest_path();

        if !manifest_path.exists() {
            return Err(PluginError::ManifestNotFound);
        }

        // Parse manifest
        let manifest = PluginManifest::from_path(&manifest_path)?;

        // Load hooks if present
        let hooks = Self::load_hooks(path)?;

        // Load MCP config if present
        let mcp_servers = Self::load_mcp_config(path)?;

        Ok(Self {
            id: Uuid::new_v4(),
            path: path.clone(),
            manifest,
            hooks,
            mcp_servers,
        })
    }

    pub(crate) fn load_hooks(
        path: &PluginPath,
    ) -> Result<Vec<HookConfig>, PluginError> {
        let hooks_path = path.hooks_path();
        if !hooks_path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&hooks_path)?;
        let raw: serde_json::Value = serde_json::from_str(&content)?;

        let mut hooks = Vec::new();
        if let Some(obj) = raw.as_object() {
            for (name, value) in obj {
                let path = value
                    .as_str()
                    .map(String::from)
                    .or_else(|| {
                        value
                            .get("path")
                            .and_then(|v| v.as_str().map(String::from))
                    })
                    .ok_or_else(|| {
                        PluginError::InvalidHooksConfig(name.clone())
                    })?;

                let timeout =
                    value.get("timeout").and_then(serde_json::Value::as_u64);

                hooks.push(HookConfig {
                    name: name.clone(),
                    path,
                    timeout_seconds: timeout,
                });
            }
        }

        Ok(hooks)
    }

    pub(crate) fn load_mcp_config(
        path: &PluginPath,
    ) -> Result<Vec<McpServerConfig>, PluginError> {
        let mcp_path = path.mcp_config_path();
        if !mcp_path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&mcp_path)?;
        let raw: serde_json::Value = serde_json::from_str(&content)?;

        let mut servers = Vec::new();
        if let Some(obj) = raw.as_object() {
            for (name, value) in obj {
                let value_obj = value.as_object().ok_or_else(|| {
                    PluginError::InvalidMcpConfig(name.clone())
                })?;

                // Parse transport (defaulting to Stdio)
                let transport = value_obj
                    .get("transport")
                    .and_then(|v| v.as_str())
                    .and_then(|t| match t {
                        "stdio" => Some(crate::types::Transport::Stdio),
                        "sse" => Some(crate::types::Transport::Sse),
                        "http" => Some(crate::types::Transport::Http),
                        "ws" => Some(crate::types::Transport::Ws),
                        _ => None,
                    });

                // Parse command (optional - network transports don't need it)
                let command = value_obj
                    .get("command")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let args = value_obj
                    .get("args")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                let mut env = std::collections::HashMap::new();
                if let Some(env_obj) =
                    value_obj.get("env").and_then(|v| v.as_object())
                {
                    for (k, v) in env_obj {
                        if let Some(val) = v.as_str() {
                            env.insert(k.clone(), val.to_string());
                        }
                    }
                }

                // Parse URL (for network transports)
                let url = value_obj
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                servers.push(McpServerConfig {
                    name: name.clone(),
                    transport,
                    command,
                    args,
                    env,
                    url,
                });
            }
        }

        Ok(servers)
    }
}
