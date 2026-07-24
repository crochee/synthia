//! Config registration and tool discovery (both eager "start to
//! discover" and lazy "discover without connecting" modes).
//!
//! `discover_tools_internal` is the lower-level helper that spawns a
//! temporary child process with `--list-tools` and parses its stdout.

use std::{collections::HashMap, process::Stdio, sync::Arc};

use super::types::McpManager;
use crate::{
    discovery::ToolDefinition,
    types::{McpError, McpServerConfig},
};

impl McpManager {
    pub async fn register_config(&self, config: McpServerConfig) {
        self.configs
            .write()
            .await
            .insert(config.name.clone(), config);
    }

    pub async fn discover_tools(
        &self,
    ) -> Result<HashMap<String, Vec<crate::types::ToolSummary>>, McpError> {
        let configs = self.configs.read().await;
        let server_names: Vec<String> = configs.keys().cloned().collect();
        drop(configs);

        let mut result = HashMap::new();
        for server_name in &server_names {
            self.start(server_name).await?;
            if let Some(discovery) = self.get_discovery(server_name).await {
                let summaries = discovery.list_summaries().await;
                result.insert(server_name.clone(), summaries);
            }
        }
        Ok(result)
    }

    pub async fn discover_tools_for_server(
        &self,
        server_name: &str,
    ) -> Result<Vec<crate::types::ToolSummary>, McpError> {
        self.start(server_name).await?;
        if let Some(discovery) = self.get_discovery(server_name).await {
            Ok(discovery.list_summaries().await)
        } else {
            Ok(vec![])
        }
    }

    /// Discover tools without establishing a full connection.
    /// Uses a temporary process with --list-tools argument for stdio MCP,
    /// or HTTP API for HTTP MCP.
    pub async fn discover_tools_fast(
        &self,
        server_name: &str,
    ) -> Result<Vec<ToolDefinition>, McpError> {
        let configs = self.configs.read().await;
        let config = configs
            .get(server_name)
            .ok_or_else(|| {
                McpError::ServerNotFound(format!(
                    "No config for '{}'",
                    server_name
                ))
            })?
            .clone();
        drop(configs);

        if self.hybrid_mode_enabled
            && let Some(tools) =
                self.discovered_tools.read().await.get(server_name)
        {
            return Ok(tools.clone());
        }

        let tools = self.discover_tools_internal(&config).await?;

        if self.hybrid_mode_enabled {
            self.discovered_tools
                .write()
                .await
                .insert(server_name.to_string(), tools.clone());
        }

        Ok(tools)
    }

    async fn discover_tools_internal(
        &self,
        config: &McpServerConfig,
    ) -> Result<Vec<ToolDefinition>, McpError> {
        use tokio::{
            io::{AsyncBufReadExt, BufReader},
            process::Command,
        };

        let mut child = Command::new(&config.command)
            .args(
                config
                    .args
                    .iter()
                    .chain(std::iter::once(&"--list-tools".to_string())),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(McpError::Connection)?;

        let stdout = child.stdout.take().ok_or_else(|| {
            McpError::ServerNotFound(format!(
                "No stdout for server '{}'",
                config.name
            ))
        })?;

        let mut reader = BufReader::new(stdout).lines();
        let mut output = String::new();

        while let Some(line) =
            reader.next_line().await.map_err(McpError::Connection)?
        {
            output.push_str(&line);
        }

        let tools: Vec<ToolDefinition> = if let Ok(parsed) =
            serde_json::from_str::<serde_json::Value>(&output)
        {
            if let Some(tools_array) =
                parsed.get("tools").and_then(|t| t.as_array())
            {
                tools_array
                    .iter()
                    .filter_map(|t| {
                        let name = t.get("name")?.as_str()?.to_string();
                        let description = t
                            .get("description")
                            .and_then(|d| d.as_str())
                            .unwrap_or("")
                            .to_string();
                        let input_schema = t
                            .get("inputSchema")
                            .cloned()
                            .unwrap_or(serde_json::json!({}));
                        Some(ToolDefinition {
                            name,
                            description,
                            input_schema,
                        })
                    })
                    .collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        tracing::debug!(
            server = %config.name,
            count = tools.len(),
            "Discovered tools without connection"
        );

        Ok(tools)
    }

    /// Get the discovery layer for a specific server (for tool calls).
    pub async fn get_discovery(
        &self,
        server_name: &str,
    ) -> Option<Arc<crate::discovery::ToolDiscovery>> {
        let connections = self.connections.read().await;
        connections.get(server_name).map(|c| c.discovery.clone())
    }
}
