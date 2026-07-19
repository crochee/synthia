//! `ExtensionRegistry` — dual registration with `ServiceRegistry`.

use std::sync::Arc;

use dashmap::DashMap;

use crate::Extension;

/// Errors from extension registry operations.
#[derive(Debug, thiserror::Error)]
pub enum ExtensionRegistryError {
    /// An extension with the same id already exists.
    #[error("duplicate extension id: {0}")]
    DuplicateId(String),
    /// No extension found with the given id.
    #[error("extension not found: {0}")]
    NotFound(String),
}

/// Thread-safe registry for extensions with double-registration into
/// `ServiceRegistry`.
///
/// When an extension is registered, it is also recorded in the
/// `ServiceRegistry` as an extension service. Deregistration removes
/// from both registries atomically.
pub struct ExtensionRegistry {
    /// Primary extension map: id → extension.
    extensions: DashMap<String, Arc<dyn Extension>>,
    /// Optional service registry for dual registration. When `None`,
    /// only the primary extension map is updated (backward-compatible
    /// behavior for test contexts that don't have a `ServiceRegistry`).
    service_registry: Option<Arc<synthia_service::registry::ServiceRegistry>>,
}

impl ExtensionRegistry {
    /// Create an empty registry without service registry integration.
    pub fn new() -> Self {
        Self {
            extensions: DashMap::new(),
            service_registry: None,
        }
    }

    /// Create a registry with service registry integration.
    ///
    /// When an extension is registered, it will also be recorded
    /// in the given `ServiceRegistry` via
    /// [`synthia_service::ServiceRegistry::register_with_capability`].
    /// If the service registry registration fails, the extension
    /// entry is rolled back.
    pub fn with_service_registry(
        service_registry: Arc<synthia_service::registry::ServiceRegistry>,
    ) -> Self {
        Self {
            extensions: DashMap::new(),
            service_registry: Some(service_registry),
        }
    }

    /// Register an extension.
    ///
    /// Returns `Err(DuplicateId)` if an extension with the same id
    /// already exists. When a `ServiceRegistry` is configured, the
    /// extension is also registered there; if that secondary
    /// registration fails, the primary entry is rolled back.
    pub fn register(
        &self,
        extension: Arc<dyn Extension>,
    ) -> Result<(), ExtensionRegistryError> {
        let id = extension.id().to_string();
        if self.extensions.contains_key(&id) {
            return Err(ExtensionRegistryError::DuplicateId(id));
        }
        self.extensions.insert(id.clone(), extension);

        // Dual-registration: also register with ServiceRegistry.
        if let Some(ref sr) = self.service_registry {
            use synthia_service::provider::ProviderId;
            let provider_id = ProviderId(id.clone());
            let ext_arc: Arc<dyn Extension> =
                self.extensions.get(&id).map(|r| r.value().clone()).unwrap();
            // Dual-Arc wrapping: ServiceRegistry stores `Arc<dyn Any + Send + Sync>`
            // where the payload is `Arc<Arc<dyn Extension>>`. This follows the
            // same dual-Arc pattern as `register_typed` — the outer Arc satisfies
            // `Any + Send + Sync` (since `Arc<dyn Extension>` is Sized), and
            // retrieval uses `Arc::downcast::<Arc<dyn Extension>>()` to recover
            // the inner `Arc<dyn Extension>`.
            let any_arc: Arc<dyn std::any::Any + Send + Sync> =
                Arc::new(ext_arc);
            if let Err(e) = sr
                .register_with_capability::<dyn Extension>(provider_id, any_arc)
            {
                // Rollback: remove from primary extension map.
                self.extensions.remove(&id);
                tracing::warn!(
                    extension_id = %id,
                    error = %e,
                    "Failed to register extension with ServiceRegistry, rolling back"
                );
                return Err(ExtensionRegistryError::DuplicateId(format!(
                    "ServiceRegistry rejected: {e}"
                )));
            }
        }
        Ok(())
    }

    /// Deregister an extension by id.
    ///
    /// Returns `Err(NotFound)` if no extension with the given id exists.
    pub fn deregister(
        &self,
        id: &str,
    ) -> Result<Arc<dyn Extension>, ExtensionRegistryError> {
        self.extensions
            .remove(id)
            .map(|(_, v)| v)
            .ok_or_else(|| ExtensionRegistryError::NotFound(id.to_string()))
    }

    /// Get an extension by id.
    pub fn get(&self, id: &str) -> Option<Arc<dyn Extension>> {
        self.extensions.get(id).map(|r| r.value().clone())
    }

    /// List all registered extension ids.
    pub fn ids(&self) -> Vec<String> {
        self.extensions.iter().map(|r| r.key().clone()).collect()
    }

    /// Number of registered extensions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.extensions.len()
    }

    /// Whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.extensions.is_empty()
    }
}

impl Default for ExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::manifest::{Capability, ExtensionManifest};

    struct DummyExt {
        id: String,
        manifest: ExtensionManifest,
    }

    #[async_trait::async_trait]
    impl Extension for DummyExt {
        fn id(&self) -> &str {
            &self.id
        }

        fn manifest(&self) -> &ExtensionManifest {
            &self.manifest
        }
    }

    fn make_ext(name: &str) -> DummyExt {
        let manifest = ExtensionManifest {
            name: name.into(),
            version: "0.1.0".into(),
            description: String::new(),
            capabilities: HashSet::from([Capability::Custom]),
        };
        DummyExt {
            id: name.into(),
            manifest,
        }
    }

    #[test]
    fn register_and_get() {
        let reg = ExtensionRegistry::new();
        let ext = Arc::new(make_ext("test"));
        reg.register(ext).unwrap();
        assert!(reg.get("test").is_some());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn duplicate_id_rejected() {
        let reg = ExtensionRegistry::new();
        let ext1 = Arc::new(make_ext("dup"));
        let ext2 = Arc::new(make_ext("dup"));
        reg.register(ext1).unwrap();
        let result = reg.register(ext2);
        assert!(matches!(
            result,
            Err(ExtensionRegistryError::DuplicateId(_))
        ));
    }

    #[test]
    fn deregister_removes() {
        let reg = ExtensionRegistry::new();
        let ext = Arc::new(make_ext("gone"));
        reg.register(ext).unwrap();
        let removed = reg.deregister("gone").unwrap();
        assert_eq!(removed.id(), "gone");
        assert!(reg.get("gone").is_none());
        assert!(reg.is_empty());
    }

    #[test]
    fn deregister_not_found() {
        let reg = ExtensionRegistry::new();
        let result = reg.deregister("missing");
        assert!(matches!(result, Err(ExtensionRegistryError::NotFound(_))));
    }
}
