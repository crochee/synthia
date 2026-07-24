//! The `Registry<HookInfo>` trait impl for [`HookRegistry`].
//!
//! `register` is intentionally rejected because hooks need
//! to be `Box<dyn AgentHook>` (not just a metadata record);
//! callers must use [`super::registry::HookRegistry::register_hook`].

use async_trait::async_trait;
use synthia_core::{Error, registry::Registry};

use super::{registry::HookRegistry, types::HookInfo};

#[async_trait]
impl Registry<HookInfo> for HookRegistry {
    type Filter = super::types::HookFilter;

    async fn register(&self, _item: HookInfo) -> Result<HookInfo, Error> {
        Err(Error::Internal(
            "Hook registration requires Box<dyn AgentHook>, use register_hook() instead"
                .to_string(),
        ))
    }

    async fn unregister(&self, name: &str) -> Result<(), Error> {
        let hook_info = self
            .hook_info
            .read()
            .map_err(|_| Error::Internal("RwLock poisoned".to_string()))?;
        let id_to_remove = hook_info
            .iter()
            .find(|(_, info)| info.name == name)
            .map(|(id, _)| *id);

        drop(hook_info);

        match id_to_remove {
            Some(id) => {
                if let Ok(mut set) = self.failed_hooks.write() {
                    set.remove(&id);
                }
                if let Ok(mut hook_info) = self.hook_info.write() {
                    hook_info.shift_remove(&id);
                }
                if let Ok(mut hooks) = self.hooks.write() {
                    hooks.shift_remove(&id);
                }
                Ok(())
            }
            None => Err(Error::NotFound(name.to_string())),
        }
    }

    async fn get(&self, name: &str) -> Result<Option<HookInfo>, Error> {
        let hook_info = self
            .hook_info
            .read()
            .map_err(|_| Error::Internal("RwLock poisoned".to_string()))?;
        Ok(hook_info.values().find(|info| info.name == name).cloned())
    }

    async fn list(
        &self,
        filter: Option<Self::Filter>,
    ) -> Result<Vec<HookInfo>, Error> {
        let filter = filter.unwrap_or_default();
        let hook_info = self
            .hook_info
            .read()
            .map_err(|_| Error::Internal("RwLock poisoned".to_string()))?;
        let hooks: Vec<HookInfo> = hook_info
            .values()
            .filter(|info| filter.accepts(info))
            .cloned()
            .collect();
        Ok(hooks)
    }
}
