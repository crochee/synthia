//! The 2 public data records carried by the MCP registry:
//!
//! - [`McpFilter`] — the `Registry<McpServerInfo>::Filter`
//!   type (filters by `transport_type` substring and/or
//!   `enabled_only`).
//! - [`McpServerInfo`] — the metadata record that backs the
//!   `Registry<McpServerInfo>` trait impl. Constructed from
//!   a [`crate::types::McpServerConfig`] via
//!   `From<&McpServerConfig>` (which auto-detects the
//!   transport from the `command` field).

use serde::{Deserialize, Serialize};
use synthia_core::registry::RegistryItem;

use crate::types::McpServerConfig;

#[derive(Debug, Clone, Default)]
pub struct McpFilter {
    pub transport_type: Option<String>,
    pub enabled_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub command: String,
    pub args: Vec<String>,
    pub enabled: bool,
}

impl RegistryItem for McpServerInfo {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }
}

impl From<&McpServerConfig> for McpServerInfo {
    fn from(config: &McpServerConfig) -> Self {
        let transport_type = if config.command.contains("npx")
            || config.command.contains("node")
        {
            "stdio"
        } else if config.command.starts_with("http")
            || config.command.starts_with("https")
        {
            "http"
        } else {
            "unknown"
        };

        Self {
            id: config.name.clone(),
            name: config.name.clone(),
            description: format!("MCP server via {} transport", transport_type),
            command: config.command.clone(),
            args: config.args.clone(),
            enabled: true,
        }
    }
}
