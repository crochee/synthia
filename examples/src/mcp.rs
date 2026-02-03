//! MCP Server Connector Module
//!
//! This module provides utilities for connecting to MCP (Model Context Protocol) servers
//! and registering their tools with the synthia agent tool registry.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::Result;
use rmcp::{
    ServiceExt,
    service::ServerSink,
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use synthia_agent::tools::{ToolRegistry, get_mcp_tools};
use tokio::process::Command;

/// Configuration for an MCP server
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

impl McpServerConfig {
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
            args: Vec::new(),
            env: HashMap::new(),
        }
    }

    pub fn args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    pub fn env(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }
}

/// Connect to an MCP server using stdio transport
/// Returns the ServerSink for tool calls
pub async fn connect_mcp_server(config: McpServerConfig) -> Result<ServerSink> {
    let client = ()
        .serve(TokioChildProcess::new(
            Command::new(&config.command).configure(|cmd| {
                cmd.args(&config.args);
                for (key, value) in &config.env {
                    cmd.env(key, value);
                }
            }),
        )?)
        .await?;

    let peer = client.peer().clone();

    // Keep the client alive by leaking it (for the lifetime of the process)
    // This is acceptable for MCP servers that run for the duration of the agent session
    #[allow(clippy::disallowed_methods)]
    std::mem::forget(client);

    Ok(peer)
}

/// Register MCP server tools to the tool registry
pub async fn register_mcp_tools(
    tool_registry: Arc<ToolRegistry>,
    server: ServerSink,
) -> Result<usize> {
    let tools = get_mcp_tools(server).await?;
    let count = tools.len();
    for tool in tools {
        tool_registry.register(Arc::new(tool)).await;
    }
    Ok(count)
}

/// Connect to the filesystem MCP server
pub async fn connect_filesystem_server(
    allowed_dirs: Vec<PathBuf>,
) -> Result<ServerSink> {
    let mut args = vec![
        "run".to_string(),
        "-p".to_string(),
        "synthia-mcp-filesystem".to_string(),
        "--".to_string(),
    ];

    for dir in &allowed_dirs {
        args.push("--allow".to_string());
        args.push(dir.display().to_string());
    }

    let config = McpServerConfig::new("filesystem", "cargo").args(args);

    connect_mcp_server(config).await
}

/// Connect to the fetch MCP server
pub async fn connect_fetch_server() -> Result<ServerSink> {
    let config = McpServerConfig::new("fetch", "cargo").args(vec![
        "run".to_string(),
        "-p".to_string(),
        "synthia-mcp-fetch".to_string(),
    ]);

    connect_mcp_server(config).await
}

/// Connect to both filesystem and fetch MCP servers and register their tools
pub async fn connect_and_register_all_mcp_tools(
    tool_registry: Arc<ToolRegistry>,
    working_dir: PathBuf,
) -> (Option<usize>, Option<usize>) {
    let mut fs_count = None;
    let mut fetch_count = None;

    // Connect filesystem MCP
    match connect_filesystem_server(vec![working_dir.clone()]).await {
        Ok(server) => {
            match register_mcp_tools(Arc::clone(&tool_registry), server).await {
                Ok(count) => {
                    tracing::info!("Registered {} filesystem tools", count);
                    fs_count = Some(count);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to register filesystem tools: {}",
                        e
                    );
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to connect filesystem MCP: {}", e);
        }
    }

    // Connect fetch MCP
    match connect_fetch_server().await {
        Ok(server) => {
            match register_mcp_tools(Arc::clone(&tool_registry), server).await {
                Ok(count) => {
                    tracing::info!("Registered {} fetch tools", count);
                    fetch_count = Some(count);
                }
                Err(e) => {
                    tracing::warn!("Failed to register fetch tools: {}", e);
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to connect fetch MCP: {}", e);
        }
    }

    (fs_count, fetch_count)
}
