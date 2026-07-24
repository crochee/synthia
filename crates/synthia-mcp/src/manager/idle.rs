//! Idle tracking + recycling + the non-hybrid idle monitor.
//!
//! The hybrid-mode equivalent (`start_idle_cleanup`) lives in
//! [`super::hybrid`] because it requires the hybrid mode flag to be
//! enabled and exercises the per-connection `McpConnection` state.

use std::{sync::Arc, time::Instant};

use super::types::McpManager;
use crate::types::McpError;

impl McpManager {
    /// Record activity to reset the idle timer for a server.
    pub async fn record_activity(&self, server_name: &str) {
        self.last_activity
            .write()
            .await
            .insert(server_name.to_string(), Instant::now());
    }

    /// Check if a server is idle beyond the configured timeout.
    pub async fn is_idle(&self, server_name: &str) -> bool {
        let last_activity = self.last_activity.read().await;
        if let Some(last) = last_activity.get(server_name) {
            last.elapsed() > self.idle_config.timeout
        } else {
            false
        }
    }

    /// Get all idle server names that should be recycled.
    pub async fn get_idle_servers(&self) -> Vec<String> {
        let last_activity = self.last_activity.read().await;
        let mut idle = Vec::new();
        for (name, last) in last_activity.iter() {
            if last.elapsed() > self.idle_config.timeout {
                idle.push(name.clone());
            }
        }
        idle
    }

    /// Recycle idle servers: detect and stop servers that have exceeded the timeout.
    pub async fn recycle_idle_servers(&self) -> Result<Vec<String>, McpError> {
        let idle_servers = self.get_idle_servers().await;
        let mut recycled = Vec::new();

        for server_name in &idle_servers {
            tracing::info!(
                "Recycling idle MCP server: {} (timeout: {:?})",
                server_name,
                self.idle_config.timeout
            );
            self.stop(server_name).await?;
            recycled.push(server_name.clone());
        }

        Ok(recycled)
    }

    /// Start the idle monitor background task.
    /// Periodically checks for idle servers and recycles them.
    pub fn start_idle_monitor(self: &Arc<Self>) {
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            let check_interval = manager.idle_config.check_interval;
            loop {
                tokio::time::sleep(check_interval).await;

                match manager.recycle_idle_servers().await {
                    Ok(recycled) if !recycled.is_empty() => {
                        tracing::info!(
                            servers = ?recycled,
                            "Recycled idle MCP servers"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "Failed to recycle idle servers"
                        );
                    }
                    _ => {}
                }
            }
        });
    }
}
