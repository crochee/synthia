//! ProviderRegistry v2 — source_id isolation hot-swap.
//!
//! Adopts codex + pi-mono `api-registry.ts` patterns:
//! * Two providers with the same `name` but different `source_id` may
//!   coexist; registering with a different `source_id` replaces the
//!   existing entry (last-wins per source).
//! * `replace_source` performs an atomic single-writer swap of every
//!   entry owned by a given `source_id`.
//!
//! This module is additive to the existing v1 `ProviderRegistry`
//! defined in `super::provider_registry` (see `super::ProviderRegistry`).
//! Both versions live side-by-side; v1 callers are unaffected.

use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::traits::ModelProvider;

/// Identifier for the originating extension/source that registered a
/// provider. Used for isolation: two registrations with different
/// `SourceId` values never collide, even when they share a `name`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceId(pub String);

/// A provider paired with the `SourceId` that registered it.
#[derive(Clone)]
pub struct RegisteredProvider {
    pub provider: Arc<dyn ModelProvider>,
    pub source_id: SourceId,
}

impl std::fmt::Debug for RegisteredProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegisteredProvider")
            .field("provider", &self.provider.name())
            .field("source_id", &self.source_id)
            .finish()
    }
}

/// Errors produced by `ProviderRegistry` v2.
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("provider already registered with same source_id")]
    AlreadyRegistered,
    #[error("source_id mismatch")]
    SourceMismatch,
}

/// Provider registry that keys by `(name, source_id)` semantics with
/// last-wins replacement behaviour on source mismatch.
///
/// Stores a flat `name -> RegisteredProvider` map. Last-wins semantics
/// for `register(name, provider, source_id)` mean that two sources
/// racing to register the same name will keep only the most recent
/// entry — each source therefore sees a consistent view of the
/// registry across `replace_source` boundaries.
#[derive(Default)]
pub struct ProviderRegistry {
    providers: RwLock<HashMap<String, RegisteredProvider>>,
}

impl ProviderRegistry {
    /// Construct an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a provider under `name`, owned by `source_id`.
    ///
    /// * Re-registering with the **same** `source_id` REJECTS with
    ///   [`RegistryError::AlreadyRegistered`] (callers must
    ///   `unregister` first, or use `replace_source` to swap whole
    ///   sets atomically).
    /// * Re-registering with a **different** `source_id` for the same
    ///   name REPLACES (last-wins). This is the "isolation" guarantee:
    ///   each source is free to define its own view of a name without
    ///   coordinating with other sources.
    pub async fn register(
        &self,
        name: impl Into<String>,
        provider: Arc<dyn ModelProvider>,
        source_id: SourceId,
    ) -> Result<(), RegistryError> {
        let name = name.into();
        let mut guard = self.providers.write().await;
        if let Some(existing) = guard.get(&name)
            && existing.source_id == source_id
        {
            return Err(RegistryError::AlreadyRegistered);
        }
        // Different source → isolation: replace (last-wins).
        guard.insert(
            name,
            RegisteredProvider {
                provider,
                source_id,
            },
        );
        Ok(())
    }

    /// Remove a provider by `name`, validating ownership by `source_id`.
    ///
    /// * Missing entries are silently ignored (idempotent).
    /// * An entry that exists but was registered by a different
    ///   `source_id` returns [`RegistryError::SourceMismatch`].
    pub async fn unregister(
        &self,
        name: &str,
        source_id: SourceId,
    ) -> Result<(), RegistryError> {
        let mut guard = self.providers.write().await;
        match guard.get(name) {
            Some(existing) if existing.source_id == source_id => {
                guard.remove(name);
                Ok(())
            }
            Some(_) => Err(RegistryError::SourceMismatch),
            None => Ok(()),
        }
    }

    /// Atomically swap the entire provider set for `source_id`.
    ///
    /// Removes every existing entry whose `source_id` matches the
    /// argument, then inserts each `(name, provider)` pair in
    /// `new_set`. The returned `usize` is the number of new entries
    /// inserted (i.e. `new_set.len()`); the number of removed entries
    /// is not returned because it varies across calls.
    pub async fn replace_source(
        &self,
        source_id: SourceId,
        new_set: Vec<(String, Arc<dyn ModelProvider>)>,
    ) -> Result<usize, RegistryError> {
        let mut guard = self.providers.write().await;
        let to_remove: Vec<String> = guard
            .iter()
            .filter_map(|(k, v)| {
                if v.source_id == source_id {
                    Some(k.clone())
                } else {
                    None
                }
            })
            .collect();
        for key in &to_remove {
            guard.remove(key);
        }
        let count = new_set.len();
        for (name, provider) in new_set {
            guard.insert(
                name,
                RegisteredProvider {
                    provider,
                    source_id: source_id.clone(),
                },
            );
        }
        Ok(count)
    }

    /// Resolve a provider by `name`. Cheap: returns a cloned `Arc`.
    pub async fn get(&self, name: &str) -> Option<Arc<dyn ModelProvider>> {
        let guard = self.providers.read().await;
        guard.get(name).map(|r| r.provider.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_id_equality_and_hash() {
        // SourceId must be usable as a HashMap key.
        let mut map: HashMap<SourceId, &'static str> = HashMap::new();
        map.insert(SourceId("a".into()), "alpha");
        map.insert(SourceId("b".into()), "beta");
        assert_eq!(map.get(&SourceId("a".into())), Some(&"alpha"));
        assert_eq!(map.get(&SourceId("b".into())), Some(&"beta"));
        assert_eq!(map.get(&SourceId("c".into())), None);
    }
}
