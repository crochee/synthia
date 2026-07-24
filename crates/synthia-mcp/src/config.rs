use std::{collections::HashMap, path::Path};

use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::types::McpServerConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(default)]
    pub mcp_servers: HashMap<String, McpServerEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerEntry {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default = "default_disabled")]
    pub disabled: bool,
}

fn default_disabled() -> bool {
    false
}

impl McpConfig {
    pub async fn load(workspace_root: &Path) -> Result<Self, std::io::Error> {
        let config_path = workspace_root.join(".agents").join("mcp.json");
        if !config_path.exists() {
            return Ok(Self {
                mcp_servers: HashMap::new(),
            });
        }

        let content = fs::read_to_string(&config_path).await?;
        let config: McpConfig =
            serde_json::from_str(&content).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, e)
            })?;

        Ok(config)
    }

    pub fn to_server_configs(&self) -> Vec<McpServerConfig> {
        self.mcp_servers
            .iter()
            .filter(|(_, entry)| !entry.disabled)
            .map(|(name, entry)| McpServerConfig {
                name: name.clone(),
                command: entry.command.clone(),
                args: entry.args.clone(),
                env: entry.env.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_config_empty() {
        let config = McpConfig {
            mcp_servers: HashMap::new(),
        };
        assert!(config.to_server_configs().is_empty());
    }

    #[test]
    fn test_mcp_config_to_server_configs() {
        let mut servers = HashMap::new();
        servers.insert(
            "filesystem".to_string(),
            McpServerEntry {
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-filesystem".to_string(),
                    "/tmp".to_string(),
                ],
                env: HashMap::new(),
                disabled: false,
            },
        );
        servers.insert(
            "disabled-server".to_string(),
            McpServerEntry {
                command: "disabled".to_string(),
                args: vec![],
                env: HashMap::new(),
                disabled: true,
            },
        );

        let config = McpConfig {
            mcp_servers: servers,
        };
        let server_configs = config.to_server_configs();

        assert_eq!(server_configs.len(), 1);
        assert_eq!(server_configs[0].name, "filesystem");
    }

    #[tokio::test]
    async fn test_mcp_config_load_nonexistent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = McpConfig::load(temp_dir.path()).await.unwrap();
        assert!(config.mcp_servers.is_empty());
    }
}
