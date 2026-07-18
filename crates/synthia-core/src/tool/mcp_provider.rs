//! McpToolProvider — tool provider backed by an MCP server connection.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;

use crate::tool::{
    descriptor::{Tool, ToolDescriptor, ToolProvenance},
    mcp_types::McpConnection,
    provider::{ToolCall, ToolEvent, ToolProvider},
    types::{ToolContext, ToolError, ToolInput, ToolOutput},
};

/// Tool provider backed by an MCP server via [`McpConnection`].
pub struct McpToolProvider {
    server_name: String,
    host_owned: bool,
    connection: Arc<dyn McpConnection>,
    /// Cached descriptors (refreshed on list_tools).
    descriptors: parking_lot::RwLock<Vec<ToolDescriptor>>,
}

impl McpToolProvider {
    pub fn new(
        server_name: String,
        host_owned: bool,
        connection: Arc<dyn McpConnection>,
    ) -> Self {
        Self {
            server_name,
            host_owned,
            connection,
            descriptors: parking_lot::RwLock::new(Vec::new()),
        }
    }

    pub fn server_name(&self) -> &str {
        &self.server_name
    }
}

#[async_trait]
impl ToolProvider for McpToolProvider {
    fn id(&self) -> &str {
        &self.server_name
    }

    async fn list_tools(&self) -> Vec<ToolDescriptor> {
        match self.connection.list_tools().await {
            Ok(tools) => {
                let mut descs: Vec<ToolDescriptor> = tools
                    .into_iter()
                    .map(|mut d| {
                        d.provenance = ToolProvenance::Mcp {
                            server: self.server_name.clone(),
                            host_owned: self.host_owned,
                        };
                        d
                    })
                    .collect();
                *self.descriptors.write() = descs.clone();
                descs
            }
            Err(_) => self.descriptors.read().clone(),
        }
    }

    async fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        // MCP tools are remote — we return a proxy that delegates to the connection.
        // For now, return None as the actual tool proxy implementation
        // requires deeper integration with the existing MCP code.
        let _ = name;
        None
    }

    async fn on_tool_event(&self, _event: &ToolEvent) {}

    async fn before_execute(&self, _call: &ToolCall) -> Result<(), ToolError> {
        Ok(())
    }

    async fn after_execute(&self, _call: &ToolCall, _result: &ToolOutput) {}
}
