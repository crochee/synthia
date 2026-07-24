//! The `Registry<McpServerInfo>` trait impl for
//! [`super::registry::McpRegistry`].

use async_trait::async_trait;
use synthia_core::{Error, registry::Registry};

use super::{
    registry::McpRegistry,
    types::{McpFilter, McpServerInfo},
};

#[async_trait]
impl Registry<McpServerInfo> for McpRegistry {
    type Filter = McpFilter;

    async fn register(
        &self,
        item: McpServerInfo,
    ) -> Result<McpServerInfo, Error> {
        let mut servers = self
            .servers
            .write()
            .map_err(|_| Error::Internal("RwLock poisoned".to_string()))?;
        if servers.contains_key(&item.id) {
            return Err(Error::AlreadyExists(item.id.clone()));
        }
        servers.insert(item.id.clone(), item.clone());
        Ok(item)
    }

    async fn unregister(&self, name: &str) -> Result<(), Error> {
        let mut servers = self
            .servers
            .write()
            .map_err(|_| Error::Internal("RwLock poisoned".to_string()))?;
        if servers.remove(name).is_none() {
            return Err(Error::NotFound(name.to_string()));
        }
        Ok(())
    }

    async fn get(&self, name: &str) -> Result<Option<McpServerInfo>, Error> {
        let servers = self
            .servers
            .read()
            .map_err(|_| Error::Internal("RwLock poisoned".to_string()))?;
        Ok(servers.get(name).cloned())
    }

    async fn list(
        &self,
        filter: Option<Self::Filter>,
    ) -> Result<Vec<McpServerInfo>, Error> {
        Ok(self.filter_servers(filter))
    }
}
