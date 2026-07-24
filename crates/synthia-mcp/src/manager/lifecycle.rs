//! Connection lifecycle: start, stop, restart, stop_all, and
//! per-server status.
//!
//! All methods in this module assume the caller already has the
//! necessary `RwLock` acquisitions in the right order; see the
//! individual doc comments for details.

use std::sync::Arc;

use rmcp::{ServiceExt, transport::TokioChildProcess};

use super::types::{McpManager, ServerConnection};
use crate::{
    discovery::ToolDiscovery,
    types::{ConnectionStatus, McpError},
};

impl McpManager {
    /// Lazy start: connect on first use, not eagerly.
    /// Spawns the MCP server process and establishes stdio transport.
    pub async fn start(&self, server_name: &str) -> Result<(), McpError> {
        // Check if already running
        {
            let conns = self.connections.read().await;
            if let Some(conn) = conns.get(server_name)
                && conn.status == ConnectionStatus::Connected
            {
                return Ok(()); // Already connected
            }
        }

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

        let mut cmd = tokio::process::Command::new(&config.command);
        cmd.args(&config.args);

        let transport = TokioChildProcess::new(&mut cmd).map_err(|e| {
            McpError::ServerNotFound(format!(
                "Failed to create rmcp transport for '{}': {}",
                server_name, e
            ))
        })?;

        let service = crate::server::McpClientService::default();

        let running_service =
            service
                .serve(transport)
                .await
                .map_err(|e: std::io::Error| {
                    McpError::ServerNotFound(format!(
                        "Failed to start rmcp client service for '{}': {}",
                        server_name, e
                    ))
                })?;

        let discovery = Arc::new(ToolDiscovery::new());

        tracing::info!(
            server = %server_name,
            "Connected to MCP server with lazy start"
        );

        let conn = ServerConnection {
            status: ConnectionStatus::Connected,
            discovery,
            running_service: Some(running_service),
            hybrid_connection: None,
        };

        self.connections
            .write()
            .await
            .insert(server_name.to_string(), conn);

        // Track activity for idle timeout
        self.last_activity
            .write()
            .await
            .insert(server_name.to_string(), std::time::Instant::now());

        Ok(())
    }

    pub async fn stop(&self, server_name: &str) -> Result<(), McpError> {
        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.get_mut(server_name) {
            conn.status = ConnectionStatus::Disconnected;
            tracing::info!(
                server = %server_name,
                "Disconnected from MCP server"
            );
        }
        connections.remove(server_name);
        self.last_activity.write().await.remove(server_name);
        Ok(())
    }

    pub async fn restart(&self, server_name: &str) -> Result<(), McpError> {
        self.stop(server_name).await?;
        self.start(server_name).await
    }

    /// Stop all connections: cleanup on agent exit.
    pub async fn stop_all(&self) -> Result<(), McpError> {
        let connections = self.connections.write().await;
        for (name, _conn) in connections.iter() {
            tracing::info!(server = %name, "Stopping MCP server on exit");
        }
        // Connections and activity tracking are cleared
        // OAuth tokens are persisted (they stay in the CredentialStore)
        drop(connections);
        // Persist OAuth tokens before exit
        let _ = self.credential_store.shutdown().await;
        Ok(())
    }

    pub async fn get_status(
        &self,
        server_name: &str,
    ) -> Option<ConnectionStatus> {
        let connections = self.connections.read().await;
        connections.get(server_name).map(|c| c.status)
    }
}
