use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EmptyFilter;

impl EmptyFilter {
    pub fn accepts<T: Send + Sync>(&self, _item: &T) -> bool {
        true
    }
}

/// Common properties that all registry items must implement.
pub trait RegistryItem: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
}

/// Basic CRUD operations for registry management.
#[async_trait]
pub trait Registry<E>: Send + Sync
where
    E: RegistryItem + Clone + Serialize + Deserialize<'static> + 'static,
{
    type Filter: Clone + Send + Sync + 'static;

    async fn register(&self, item: E) -> Result<E, Error>;
    async fn unregister(&self, name: &str) -> Result<(), Error>;
    async fn get(&self, name: &str) -> Result<Option<E>, Error>;
    async fn list(&self, filter: Option<Self::Filter>)
    -> Result<Vec<E>, Error>;
}

/// Lifecycle management for registries that need start/stop operations.
#[async_trait]
pub trait LifecycleRegistry<E>: Registry<E>
where
    E: RegistryItem + Clone + Serialize + Deserialize<'static> + 'static,
{
    async fn start(&self, name: &str) -> Result<(), Error>;
    async fn stop(&self, name: &str) -> Result<(), Error>;
    async fn stop_all(&self) -> Result<(), Error>;
}
