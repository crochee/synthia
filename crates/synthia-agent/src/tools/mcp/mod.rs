//! MCP tool adapter module
//!
//! This module provides adapters for MCP (Model Context Protocol) tools.

mod adapter;
pub mod mcp_auth;
pub mod mcp_resource_tools;
pub mod remote_trigger;

#[cfg(test)]
mod tests;

use std::{collections::HashMap, sync::Arc};

pub use adapter::McpToolAdapter;
pub use mcp_auth::McpAuthTool;
pub use mcp_resource_tools::{ListMcpResourcesTool, ReadMcpResourceTool};
pub use remote_trigger::RemoteTriggerTool;
use rmcp::{
    model::{ReadResourceRequestParams, Resource},
    service::ServerSink,
};
use tokio::sync::RwLock;

use crate::AgentError;

/// Collects tools from multiple MCP servers
#[derive(Clone)]
pub struct McpToolCollector {
    servers: Arc<RwLock<HashMap<String, ServerSink>>>,
}

impl Default for McpToolCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl McpToolCollector {
    pub fn new() -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_server(&self, id: String, server: ServerSink) {
        let mut servers = self.servers.write().await;
        servers.insert(id, server);
    }

    pub async fn unregister_server(&self, id: &str) {
        let mut servers = self.servers.write().await;
        servers.remove(id);
    }

    pub async fn collect_all_tools(
        &self,
    ) -> Result<Vec<McpToolAdapter>, AgentError> {
        let servers = self.servers.read().await;
        let mut all_tools = Vec::new();

        for (server_id, server) in servers.iter() {
            let tools = server.list_all_tools().await.map_err(|e| {
                AgentError::InvalidOperation(format!(
                    "Failed to list MCP tools from {server_id}: {e}"
                ))
            })?;
            for tool in tools {
                all_tools.push(McpToolAdapter::new(tool, server.clone()));
            }
        }

        Ok(all_tools)
    }

    /// Parse qualified name "mcp__server__tool" -> (server_id, tool_name)
    pub fn parse_qualified_name(name: &str) -> Option<(String, String)> {
        let prefix = "mcp__";
        if !name.starts_with(prefix) {
            return None;
        }
        let remainder = &name[prefix.len()..];
        if let Some(pos) = remainder.find("__") {
            let server = remainder[..pos].to_string();
            let tool = remainder[pos + 2..].to_string();
            Some((server, tool))
        } else {
            None
        }
    }

    /// List all connected server names
    pub async fn list_all_servers(&self) -> Vec<String> {
        let servers = self.servers.read().await;
        servers.keys().cloned().collect()
    }

    /// List all resources from a specific server
    pub async fn list_server_resources(
        &self,
        server_name: &str,
    ) -> Result<Vec<Resource>, AgentError> {
        let servers = self.servers.read().await;
        let server = servers.get(server_name).ok_or_else(|| {
            AgentError::InvalidOperation(format!(
                "Server '{server_name}' not found"
            ))
        })?;

        server.list_all_resources().await.map_err(|e| {
            AgentError::InvalidOperation(format!(
                "Failed to list resources: {e}"
            ))
        })
    }

    /// Read a resource from a specific server
    pub async fn read_server_resource(
        &self,
        server_name: &str,
        uri: &str,
    ) -> Result<Vec<rmcp::model::ResourceContents>, AgentError> {
        use rmcp::model::ReadResourceResult;

        let servers = self.servers.read().await;
        let server = servers.get(server_name).ok_or_else(|| {
            AgentError::InvalidOperation(format!(
                "Server '{server_name}' not found"
            ))
        })?;

        let result: ReadResourceResult = server
            .read_resource(ReadResourceRequestParams {
                meta: None,
                uri: uri.to_string(),
            })
            .await
            .map_err(|e| {
                AgentError::InvalidOperation(format!(
                    "Failed to read resource: {e}"
                ))
            })?;

        Ok(result.contents)
    }
}

pub async fn get_mcp_tools(
    server: ServerSink,
) -> Result<Vec<McpToolAdapter>, AgentError> {
    let tools = server.list_all_tools().await.map_err(|e| {
        AgentError::InvalidOperation(format!("Failed to list MCP tools: {e}"))
    })?;
    Ok(tools
        .into_iter()
        .map(|tool| McpToolAdapter::new(tool, server.clone()))
        .collect())
}
