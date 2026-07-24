//! The `LifecycleRegistry<McpServerInfo>` trait impl for
//! [`super::registry::McpRegistry`]. All 3 methods delegate
//! to the optional [`crate::manager::McpManager`] and return
//! `Error::Internal` if no manager is configured.

use async_trait::async_trait;
use synthia_core::{Error, registry::LifecycleRegistry};

use super::{registry::McpRegistry, types::McpServerInfo};

#[async_trait]
impl LifecycleRegistry<McpServerInfo> for McpRegistry {
    async fn start(&self, name: &str) -> Result<(), Error> {
        if let Some(ref manager) = self.manager {
            manager.start(name).await.map_err(|e| {
                Error::Internal(format!("Failed to start server: {}", e))
            })
        } else {
            Err(Error::Internal("No manager configured".to_string()))
        }
    }

    async fn stop(&self, name: &str) -> Result<(), Error> {
        if let Some(ref manager) = self.manager {
            manager.stop(name).await.map_err(|e| {
                Error::Internal(format!("Failed to stop server: {}", e))
            })
        } else {
            Err(Error::Internal("No manager configured".to_string()))
        }
    }

    async fn stop_all(&self) -> Result<(), Error> {
        if let Some(ref manager) = self.manager {
            manager.stop_all().await.map_err(|e| {
                Error::Internal(format!("Failed to stop all servers: {}", e))
            })
        } else {
            Err(Error::Internal("No manager configured".to_string()))
        }
    }
}
