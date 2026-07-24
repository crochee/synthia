//! The 5 stop/status methods on [`super::core::McpProxy`]:
//!
//! - [`McpProxy::stop_server`] — removes the handle from the
//!   map, then kills the stdio `Child` (best-effort; the
//!   `ServerAlreadyRunning` check is a separate path) or
//!   marks a network handle as inactive.
//! - [`McpProxy::stop_all`] — iterates a snapshot of the
//!   keys and calls `stop_server` for each (errors are
//!   logged, not propagated).
//! - [`McpProxy::shutdown`] — alias for `stop_all`,
//!   documented as the preferred way to stop the proxy
//!   before drop.
//! - [`McpProxy::running_servers`] / `is_running` — pure
//!   read accessors.

use tracing::{debug, error, info};

use super::{core::McpProxy, error::McpProxyError, handle::ServerHandle};

impl McpProxy {
    /// Stop a single MCP server by name
    pub async fn stop_server(&self, name: &str) -> Result<(), McpProxyError> {
        let handle = {
            let mut servers = self.servers.write().await;
            servers.remove(name)
        };

        match handle {
            Some(ServerHandle::Stdio(mut child)) => {
                info!("stopping stdio server: {}", name);
                child.kill().await.map_err(|e| {
                    McpProxyError::StopFailed(name.to_string(), e)
                })?;
                debug!("stdio server {} stopped", name);
            }
            Some(ServerHandle::Network(handle)) => {
                info!(
                    "stopping network server: {} (transport: {:?})",
                    name, handle.transport
                );
                // Mark as inactive - actual cleanup depends on transport type
                debug!("network server {} stopped", name);
            }
            None => {
                return Err(McpProxyError::ServerNotFound(name.to_string()));
            }
        }

        Ok(())
    }

    /// Stop all managed MCP servers
    pub async fn stop_all(&self) -> Result<(), McpProxyError> {
        let names: Vec<String> = {
            let servers = self.servers.read().await;
            servers.keys().cloned().collect()
        };

        for name in names {
            if let Err(e) = self.stop_server(&name).await {
                error!("failed to stop server {}: {}", name, e);
            }
        }

        Ok(())
    }

    /// Shutdown the proxy and stop all servers.
    ///
    /// This is the preferred way to stop the proxy since [`Drop`] cannot run async
    /// cleanup. Callers should invoke this method before dropping the proxy.
    pub async fn shutdown(&self) -> Result<(), McpProxyError> {
        self.stop_all().await
    }

    /// Get the names of all running servers
    pub async fn running_servers(&self) -> Vec<String> {
        let servers = self.servers.read().await;
        servers.keys().cloned().collect()
    }

    /// Check if a server is running
    pub async fn is_running(&self, name: &str) -> bool {
        let servers = self.servers.read().await;
        servers.contains_key(name)
    }
}
