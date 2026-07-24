//! The four data structs/types that describe a plugin's static
//! configuration: [`PluginId`], [`PluginPath`], [`HookConfig`], and
//! [`McpServerConfig`]. These are the units the loader reads from
//! disk and the runtime uses to dispatch hooks / MCP servers.

use std::path::{Path, PathBuf};

use uuid::Uuid;

/// Unique identifier for a plugin (UUID v4)
pub type PluginId = Uuid;

/// Absolute path to a plugin directory
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginPath(PathBuf);

impl PluginPath {
    /// Create a new PluginPath from a PathBuf
    pub fn new(path: PathBuf) -> Self {
        Self(path)
    }

    /// Get the underlying path reference
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Get the manifest path for this plugin
    pub fn manifest_path(&self) -> PathBuf {
        self.0.join("plugin.json")
    }

    /// Get the hooks config path for this plugin
    pub fn hooks_path(&self) -> PathBuf {
        self.0.join("hooks.json")
    }

    /// Get the MCP server config path for this plugin
    pub fn mcp_config_path(&self) -> PathBuf {
        self.0.join("mcp.json")
    }
}

impl std::fmt::Display for PluginPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

/// Hook configuration loaded from hooks.json
#[derive(Debug, Clone, serde::Deserialize)]
pub struct HookConfig {
    /// Hook name (e.g., "pre-task", "post-task")
    pub name: String,
    /// Path to the hook script
    pub path: String,
    /// Optional timeout in seconds
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

/// MCP server configuration loaded from mcp.json
#[derive(Debug, Clone, serde::Deserialize)]
pub struct McpServerConfig {
    /// Server name
    pub name: String,
    /// Transport mode
    #[serde(default)]
    pub transport: Option<crate::types::Transport>,
    /// Command to run (for stdio transport)
    pub command: Option<String>,
    /// Command arguments (for stdio transport)
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables (for stdio transport)
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    /// URL endpoint (for sse/http/ws transports)
    #[serde(default)]
    pub url: Option<String>,
}

impl McpServerConfig {
    /// Returns the transport mode, defaulting to Stdio
    pub fn transport(&self) -> crate::types::Transport {
        self.transport.unwrap_or(crate::types::Transport::Stdio)
    }

    /// Validate that this config has required fields for its transport type
    pub fn validate(&self) -> Result<(), crate::types::McpConfigError> {
        match self.transport() {
            crate::types::Transport::Stdio => {
                if self.command.is_none()
                    || self.command.as_ref().is_some_and(String::is_empty)
                {
                    return Err(crate::types::McpConfigError::MissingCommand);
                }
            }
            crate::types::Transport::Sse
            | crate::types::Transport::Http
            | crate::types::Transport::Ws => {
                if self.url.is_none()
                    || self.url.as_ref().is_some_and(String::is_empty)
                {
                    return Err(crate::types::McpConfigError::MissingUrl(
                        self.transport(),
                    ));
                }
            }
        }
        Ok(())
    }
}
