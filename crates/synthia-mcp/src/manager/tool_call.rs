//! Tool calling: the production call path (`call_tool`),
//! [`is_connected`] checks, the [`with_running_service_mut`] helper for
//! ad-hoc access to the running rmcp service, and the credential store
//! accessor.

use std::sync::Arc;

use rmcp::service::{RoleClient, RunningService};

use super::types::McpManager;
use crate::{
    oauth::CredentialStore,
    types::{ConnectionStatus, McpError},
};

impl McpManager {
    /// Get a mutable reference to the running service for a server via a closure.
    /// Used when making tool calls or loading schemas.
    pub async fn with_running_service_mut<F, R>(
        &self,
        server_name: &str,
        f: F,
    ) -> Option<R>
    where
        F: FnOnce(
            &mut RunningService<RoleClient, crate::server::McpClientService>,
        ) -> R,
    {
        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.get_mut(server_name)
            && let Some(ref mut running_service) = conn.running_service
        {
            return Some(f(running_service));
        }
        None
    }

    /// Get the credential store reference.
    pub fn credential_store(&self) -> &Arc<CredentialStore> {
        &self.credential_store
    }

    /// Call a tool on a connected MCP server.
    /// Returns the raw JSON result from the MCP server.
    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        // Check if server is connected
        {
            let conns = self.connections.read().await;
            let conn = conns.get(server_name).ok_or_else(|| {
                McpError::ServerNotFound(format!(
                    "MCP server '{}' not connected",
                    server_name
                ))
            })?;
            if conn.status != ConnectionStatus::Connected {
                return Err(McpError::ServerNotFound(format!(
                    "MCP server '{}' is not connected",
                    server_name
                )));
            }
        }

        // Record activity for idle tracking
        self.record_activity(server_name).await;

        // Execute the call using the running service
        let mut connections = self.connections.write().await;
        let conn = connections.get_mut(server_name).ok_or_else(|| {
            McpError::ServerNotFound(format!(
                "MCP server '{}' not connected",
                server_name
            ))
        })?;

        let hybrid_conn = conn.hybrid_connection.as_mut().ok_or_else(|| {
            McpError::ServerNotFound(format!(
                "No hybrid connection for server '{}'",
                server_name
            ))
        })?;

        let peer = hybrid_conn.peer.as_mut().ok_or_else(|| {
            McpError::ServerNotFound(format!(
                "No peer for server '{}'",
                server_name
            ))
        })?;
        let response = crate::client::call_tool(peer, tool_name, arguments)
            .await
            .map_err(|e| {
                McpError::ServerNotFound(format!("Tool call failed: {}", e))
            })?;
        let content = serde_json::to_value(&response.content).map_err(|e| {
            McpError::ServerNotFound(format!(
                "Failed to serialize content: {}",
                e
            ))
        })?;

        Ok(content)
    }

    /// Check if a server is connected (for use by McpTool).
    pub async fn is_connected(&self, server_name: &str) -> bool {
        let conns = self.connections.read().await;
        conns
            .get(server_name)
            .map(|c| c.status == ConnectionStatus::Connected)
            .unwrap_or(false)
    }
}
