//! The [`HookRegistry`] struct + its `Default` impl + the
//! lifecycle methods (`new` / `register_hook` /
//! `unregister_by_handle` / `len` / `is_empty` / `contains`
//! / `is_failed` / `record_failure`).
//!
//! The 6 `fire_*` methods live in [`super::fire`] (they
//! share the "iterate non-failed hooks + safe-fire" pattern,
//! and a single file keeps that loop consistent). The
//! `Registry<HookInfo>` trait impl lives in
//! [`super::registry_trait`].

use std::{collections::HashSet, sync::RwLock};

use indexmap::IndexMap;
use ulid::Ulid;

use super::{safety::get_hook_name, types::HookInfo};
use crate::{hook_trait::AgentHookAdapter, traits::AgentHook};

pub struct HookRegistry {
    pub(super) hooks: RwLock<IndexMap<Ulid, Arc<dyn AgentHook>>>,
    pub(super) hook_info: RwLock<IndexMap<Ulid, HookInfo>>,
    pub(super) failed_hooks: RwLock<HashSet<Ulid>>,
}

use std::sync::Arc;

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            hooks: RwLock::new(IndexMap::new()),
            hook_info: RwLock::new(IndexMap::new()),
            failed_hooks: RwLock::new(HashSet::new()),
        }
    }

    pub fn register_hook(
        &self,
        hook: Box<dyn AgentHook>,
    ) -> super::types::HookHandle {
        let id = Ulid::new();
        let hook_name = get_hook_name(hook.as_ref());
        let info = HookInfo {
            id: id.to_string(),
            name: hook_name,
            description: "Agent hook".to_string(),
        };
        if let Ok(mut hooks) = self.hooks.write() {
            hooks.insert(id, Arc::from(hook));
        }
        if let Ok(mut hook_info) = self.hook_info.write() {
            hook_info.insert(id, info);
        }
        super::types::HookHandle(id)
    }

    pub fn unregister_by_handle(
        &self,
        handle: &super::types::HookHandle,
    ) -> bool {
        if let Ok(mut set) = self.failed_hooks.write() {
            set.remove(&handle.0);
        }
        let mut removed = false;
        if let Ok(mut hook_info) = self.hook_info.write() {
            hook_info.shift_remove(&handle.0);
        }
        if let Ok(mut hooks) = self.hooks.write() {
            removed = hooks.shift_remove(&handle.0).is_some();
        }
        removed
    }

    pub fn len(&self) -> usize {
        self.hooks.read().map(|h| h.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.hooks.read().map(|h| h.is_empty()).unwrap_or(true)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.hook_info
            .read()
            .map(|h| h.values().any(|info| info.name == name))
            .unwrap_or(false)
    }

    pub(super) fn is_failed(&self, id: &Ulid) -> bool {
        self.failed_hooks
            .read()
            .map(|s| s.contains(id))
            .unwrap_or(false)
    }

    pub(super) fn record_failure(&self, id: Ulid) {
        if let Ok(mut set) = self.failed_hooks.write() {
            set.insert(id);
        }
    }

    /// Snapshot all non-failed hooks, wrapping each in an
    /// [`AgentHookAdapter`] so they implement the new [`crate::Hook`]
    /// trait. Used by [`crate::UnifiedHookDispatcher::from_hook_registry`].
    pub fn snapshot_adapted_hooks(&self) -> Vec<Arc<dyn crate::Hook>> {
        let Ok(hooks) = self.hooks.read() else {
            return Vec::new();
        };
        let Ok(failed) = self.failed_hooks.read() else {
            return Vec::new();
        };
        hooks
            .iter()
            .filter(|(id, _)| !failed.contains(id))
            .map(|(_, hook)| {
                Arc::new(AgentHookAdapter::new(hook.clone()))
                    as Arc<dyn crate::Hook>
            })
            .collect()
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}
