//! Hybrid mode: lazy-connection tool calls, per-server hybrid state
//! introspection, the discovered-tools cache, the hybrid idle cleanup
//! background task, and the runtime hybrid-mode toggle.

use std::sync::Arc;

use super::types::McpManager;
use crate::{
    connection::{ConnectionState, McpConnection},
    discovery::ToolDefinition,
    types::{ConnectionStatus, McpError},
};

impl McpManager {
    /// Ensure connection is established for a server (lazy connection).
    pub async fn ensure_connected(
        &self,
        server_name: &str,
    ) -> Result<(), McpError> {
        if self.is_connected(server_name).await {
            return Ok(());
        }

        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.get_mut(server_name)
            && conn.status == ConnectionStatus::Connected
        {
            return Ok(());
        }
        drop(connections);

        self.start(server_name).await
    }

    /// Get a hybrid connection state for a server.
    pub async fn get_hybrid_connection_state(
        &self,
        server_name: &str,
    ) -> Option<ConnectionState> {
        let connections = self.connections.read().await;
        connections
            .get(server_name)
            .and_then(|c| c.hybrid_connection.as_ref())
            .map(|c| c.state())
    }

    /// Check if hybrid connection is connected for a server.
    pub async fn is_hybrid_connection_connected(
        &self,
        server_name: &str,
    ) -> bool {
        self.get_hybrid_connection_state(server_name)
            .await
            .map(|s| s == ConnectionState::Connected)
            .unwrap_or(false)
    }

    /// Connect a hybrid connection for a server (lazy connection).
    pub async fn connect_hybrid_connection(
        &self,
        server_name: &str,
    ) -> Result<(), McpError> {
        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.get_mut(server_name)
            && let Some(ref mut hybrid_conn) = conn.hybrid_connection
        {
            hybrid_conn.connect().await?;
            return Ok(());
        }
        Err(McpError::ServerNotFound(format!(
            "No hybrid connection for server '{}'",
            server_name
        )))
    }

    /// Call tool via hybrid connection.
    pub async fn call_tool_hybrid(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.get_mut(server_name)
            && let Some(ref mut hybrid_conn) = conn.hybrid_connection
        {
            return hybrid_conn.call_tool(tool_name, arguments).await;
        }
        Err(McpError::ServerNotFound(format!(
            "No hybrid connection for server '{}'",
            server_name
        )))
    }

    /// Create a hybrid connection for a server with discovered tools.
    pub async fn create_hybrid_connection(
        &self,
        server_name: &str,
        tools: Vec<ToolDefinition>,
    ) -> Result<(), McpError> {
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

        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.get_mut(server_name) {
            if conn.hybrid_connection.is_some() {
                return Ok(());
            }
            conn.hybrid_connection = Some(McpConnection::new(
                server_name.to_string(),
                config,
                tools,
            ));
            tracing::info!(
                server = %server_name,
                "Created hybrid connection"
            );
        }
        Ok(())
    }

    /// Get discovered tools for a server (hybrid mode).
    pub async fn get_discovered_tools(
        &self,
        server_name: &str,
    ) -> Option<Vec<ToolDefinition>> {
        self.discovered_tools.read().await.get(server_name).cloned()
    }

    /// Clear discovered tools for a server.
    pub async fn clear_discovered_tools(&self, server_name: &str) {
        self.discovered_tools.write().await.remove(server_name);
    }

    /// Get connection state for a server (hybrid mode).
    pub async fn get_connection_state(
        &self,
        server_name: &str,
    ) -> Option<ConnectionState> {
        let connections = self.connections.read().await;
        connections
            .get(server_name)
            .and_then(|c| c.hybrid_connection.as_ref())
            .map(|c| c.state())
    }

    /// Check if hybrid mode is enabled.
    pub fn is_hybrid_mode_enabled(&self) -> bool {
        self.hybrid_mode_enabled
    }

    /// Set hybrid mode enabled state.
    pub fn set_hybrid_mode(&mut self, enabled: bool) {
        self.hybrid_mode_enabled = enabled;
    }

    /// Start the idle cleanup task for hybrid mode connections.
    /// Periodically disconnects idle connections beyond the timeout.
    pub fn start_idle_cleanup(self: &Arc<Self>) {
        if !self.hybrid_mode_enabled {
            tracing::debug!("Hybrid mode not enabled, skipping idle cleanup");
            return;
        }

        let manager = Arc::clone(self);
        tokio::spawn(async move {
            let cleanup_interval = manager.cleanup_interval;
            let idle_timeout = manager.idle_timeout;

            loop {
                tokio::time::sleep(cleanup_interval).await;

                let mut connections = manager.connections.write().await;
                for conn in connections.values_mut() {
                    if let Some(ref mut hybrid_conn) = conn.hybrid_connection
                        && hybrid_conn.state() == ConnectionState::Connected
                        && hybrid_conn.last_used_duration() > idle_timeout
                    {
                        tracing::info!(
                            server = %hybrid_conn.server_id,
                            idle_seconds = hybrid_conn.last_used_duration().as_secs(),
                            "Disconnecting idle hybrid connection"
                        );
                        hybrid_conn.disconnect().await;
                    }
                }
            }
        });
    }
}
