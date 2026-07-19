//! ServiceRegistry — dual-indexed (TypeId + String) dynamic registry.
//!
//! Typed resolution uses `TypeId::of::<Arc<dyn SubTrait>>()`.
//! The `type_index` stores `Arc<dyn Any + Send + Sync>` payloads
//! (constructed by the caller as `Arc::new(Arc::<dyn SubTrait>::new(...))`),
//! while the `name_index` stores `Arc<dyn Service>` for diagnostics.
//!
//! PR-3.2 adds `Capability<T>` contract (see [`capability`](crate::capability)).
//! PR-3.3 adds reverse-dependency tracking (see [`reverse_dep`](crate::reverse_dep)).
//! PR-3.4 adds peer-source registration (see [`peer_source`](crate::peer_source)).

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Arc,
};

use parking_lot::RwLock;

use crate::{
    capability::{Capability, CapabilityIndex},
    output_bound::{OutputBoundService, ServiceRegistryError},
    peer_source::{CapsuleId, PeerSourceIndex, Source, StreamId},
    provider::{
        ProviderId,
        RegistrationError,
        RegistrationToken,
        ServiceDescriptor,
    },
    reverse_dep::{ReverseDepGraph, ServiceId},
    traits::{Service, ServiceError, ServiceKey, ServiceState},
};

/// Dual-indexed registry for service resolution.
pub struct ServiceRegistry {
    /// TypeId → Arc<dyn Any + Send + Sync> for typed O(1) resolution.
    /// The caller wraps `Arc<dyn SubTrait>` in `Arc::new(...)` so
    /// `TypeId::of::<Arc<Arc<dyn SubTrait>>>()` is the key and
    /// `Arc::downcast::<Arc<Arc<dyn SubTrait>>>` recovers it.
    type_index: RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
    /// String → Arc<dyn Service> for name-based diagnostics.
    name_index: RwLock<HashMap<String, Arc<dyn Service>>>,
    /// TypeId → Arc<dyn Any + Send + Sync> for the PR-3.1
    /// `OutputBoundService` lookup path. Keyed by
    /// `TypeId::of::<T::Service>()` where `T` is the concrete
    /// `OutputBoundService` impl; value wraps `Arc<dyn T::Service>`
    /// so `Arc::downcast::<Arc<dyn T::Service>>` recovers it on read.
    bound_index: RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
    /// PR-3.2: typed `Capability<T>` → `ProviderId` index.
    capability_index: CapabilityIndex,
    /// PR-3.3: reverse-dependency graph.
    reverse_dep: ReverseDepGraph,
    /// PR-3.4: peer-source (CapsuleId/StreamId) index.
    peer_source: PeerSourceIndex,
    /// Monotonic counter for registration tokens.
    next_token: RwLock<u64>,
}

impl ServiceRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            type_index: RwLock::new(HashMap::new()),
            name_index: RwLock::new(HashMap::new()),
            bound_index: RwLock::new(HashMap::new()),
            capability_index: CapabilityIndex::new(),
            reverse_dep: ReverseDepGraph::new(),
            peer_source: PeerSourceIndex::new(),
            next_token: RwLock::new(1),
        }
    }

    /// Register a typed service for both TypeId and name resolution.
    ///
    /// The caller provides:
    /// - `typed_payload`: `Arc<dyn Any + Send + Sync>` wrapping an `Arc<dyn SubTrait>`.
    ///   Construct as: `Arc::new(Arc::new(MyServiceImpl) as Arc<dyn SubTrait>)`
    /// - `expected_type_id`: `TypeId::of::<Arc<Arc<dyn SubTrait>>>()`
    /// - `name_payload`: `Arc<dyn Service>` for name-based resolution
    /// - `descriptor`: service metadata
    pub fn register_typed(
        &self,
        typed_payload: Arc<dyn Any + Send + Sync>,
        expected_type_id: TypeId,
        name_payload: Arc<dyn Service>,
        descriptor: ServiceDescriptor,
    ) -> Result<RegistrationToken, RegistrationError> {
        let token = {
            let mut next = self.next_token.write();
            let t = RegistrationToken(*next);
            *next += 1;
            t
        };

        let actual_type_id = (*typed_payload).type_id();

        // Validate TypeId consistency under debug_assertions
        #[cfg(debug_assertions)]
        {
            debug_assert_eq!(
                actual_type_id, expected_type_id,
                "TypeId mismatch for service '{}': the Any payload type \
                 must match the expected TypeId",
                descriptor.name
            );
        }

        self.type_index
            .write()
            .insert(expected_type_id, typed_payload);
        self.name_index
            .write()
            .insert(descriptor.name, name_payload);

        Ok(token)
    }

    /// Register a service by name only (no TypeId lookup).
    pub fn register_by_name(
        &self,
        service: Arc<dyn Service>,
        descriptor: ServiceDescriptor,
    ) -> Result<RegistrationToken, RegistrationError> {
        let token = {
            let mut next = self.next_token.write();
            let t = RegistrationToken(*next);
            *next += 1;
            t
        };

        self.name_index.write().insert(descriptor.name, service);

        Ok(token)
    }

    /// Register a service provider (bulk registration).
    pub async fn register_provider(
        &self,
        provider: Arc<dyn crate::provider::ServiceProvider>,
    ) -> Result<Vec<RegistrationToken>, RegistrationError> {
        let descriptors = provider.list_services().await;
        let mut tokens = Vec::with_capacity(descriptors.len());

        for desc in descriptors {
            if let Some(svc) = provider.get_service(&desc.name).await {
                let token = self.register_by_name(svc, desc)?;
                tokens.push(token);
            }
        }

        Ok(tokens)
    }

    /// Unregister all services owned by a token.
    pub fn unregister(&self, _token: RegistrationToken) {
        // TODO: implement token-based unregistration
    }

    /// Typed resolution via TypeId.
    ///
    /// Returns `Arc<dyn SubTrait>` by downcasting the stored `Any` payload.
    /// ```ignore
    /// // Registration used Arc::new(svc as Arc<dyn SessionService>)
    /// // with key TypeId::of::<Arc<Arc<dyn SessionService>>>()
    /// let service: Arc<Arc<dyn SessionService>> = registry.get_typed::<Arc<Arc<dyn SessionService>>>()?;
    /// let svc: Arc<dyn SessionService> = (*service).clone();
    /// ```
    pub fn get_typed<T>(&self, type_id: TypeId) -> Option<Arc<T>>
    where
        T: Any + Send + Sync + 'static,
    {
        let index = self.type_index.read();
        let entry = index.get(&type_id)?;
        Arc::downcast::<T>(entry.clone()).ok()
    }

    /// Bind a `Service` that implements [`OutputBoundService`] under
    /// the capability view `T = Self::Service`.
    ///
    /// Stored under `TypeId::of::<T>()` in `bound_index`. Calling
    /// `bind` twice with the same `T` replaces the previous entry (the
    /// second `Arc<S>` wins). The downstream
    /// [`bound_service::<T>()`](Self::bound_service) lookup walks the
    /// same key and downcasts to the typed `Arc`.
    pub fn bind<S, T>(
        &self,
        service: std::sync::Arc<S>,
    ) -> Result<(), ServiceRegistryError>
    where
        S: OutputBoundService<Service = T> + Service + Send + Sync + 'static,
        T: ?Sized + Send + Sync + 'static,
    {
        let view: std::sync::Arc<T> = service.as_bound();
        let payload: Arc<dyn Any + Send + Sync> = Arc::new(view);
        self.bound_index.write().insert(TypeId::of::<T>(), payload);
        Ok(())
    }

    /// Look up a service by its bound capability view.
    ///
    /// Returns `Ok(Arc<T>)` if a previous [`bind`](Self::bind) call
    /// stored an `S: OutputBoundService<Service = T>` entry. Returns
    /// `Err(ServiceRegistryError::NotBound)` otherwise. The `T: Send +
    /// Sync + 'static` bound is enforced at the call site — the
    /// compiler rejects any `T` that doesn't satisfy it (spec
    /// scenario "bind to dyn-incompatible type rejected").
    pub fn bound_service<T>(&self) -> Result<Arc<T>, ServiceRegistryError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        let index = self.bound_index.read();
        let entry = index.get(&TypeId::of::<T>()).ok_or(
            ServiceRegistryError::NotBound(std::any::type_name::<T>()),
        )?;
        // Stored payload is `Arc::new(arc: Arc<T>)`, so we downcast to
        // `Arc<Arc<T>>` and unwrap the outer Arc via clone to recover
        // the inner `Arc<T>` (mirrors the dual-Arc pattern in
        // `register_typed`).
        let wrapped: Arc<Arc<T>> = Arc::downcast::<Arc<T>>(entry.clone())
            .map_err(|_| {
                ServiceRegistryError::NotBound(std::any::type_name::<T>())
            })?;
        Ok((*wrapped).clone())
    }

    // ── PR-3.2: Capability<T> contract ──────────────────────

    /// Register a service with a typed capability declaration.
    ///
    /// The `payload` should be `Arc::new(Arc<dyn T>)` wrapping the
    /// service's capability view. The `provider_id` identifies the
    /// registering provider. The `Capability::of::<T>()` marker
    /// determines the registry key.
    ///
    /// Returns `Err(ServiceRegistryError::CapabilityMismatch)` if
    /// the payload's actual `TypeId` does not match `Arc<T>`.
    pub fn register_with_capability<T>(
        &self,
        provider_id: ProviderId,
        payload: Arc<dyn Any + Send + Sync>,
    ) -> Result<(), ServiceRegistryError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        let cap = Capability::<T>::of();
        self.capability_index.register(provider_id, payload, &cap)
    }

    /// Query all providers that declared capability `T`.
    ///
    /// Returns a `Vec<ProviderId>` of providers that previously called
    /// [`register_with_capability::<T>()`](Self::register_with_capability).
    pub fn capabilities_provided<T>(&self) -> Vec<ProviderId>
    where
        T: ?Sized + 'static,
    {
        let cap = Capability::<T>::of();
        self.capability_index.providers(&cap)
    }

    // ── PR-3.3: Reverse-dependency tracking ─────────────────

    /// Record a dependency edge: `dependent` depends on `dependency`.
    ///
    /// Performs cycle detection. If adding this edge would create a
    /// cycle, returns `Err(ServiceRegistryError::Cycle)`.
    pub fn add_dependency(
        &self,
        dependent: ServiceId,
        dependency: ServiceId,
    ) -> Result<(), ServiceRegistryError> {
        self.reverse_dep.add_edge(dependent, dependency)
    }

    /// Return all services that depend on `id` (reverse lookup).
    ///
    /// Useful for diagnostics and tooling to understand the dependency
    /// graph without introducing a runtime broker.
    pub fn reverse_dependents_of(
        &self,
        id: &ServiceId,
    ) -> std::collections::BTreeSet<ServiceId> {
        self.reverse_dep.reverse_dependents_of(id)
    }

    /// Remove a service from the reverse-dependency graph.
    pub fn remove_from_dep_graph(&self, id: &ServiceId) {
        self.reverse_dep.remove_service(id);
    }

    // ── PR-3.4: Peer-source registration ─────────────────────

    /// Register a service with a peer source tag.
    ///
    /// The `source` (CapsuleId or StreamId) scopes the service's
    /// lifetime. The `payload` must be `Arc::new(Arc<dyn T>)` wrapping
    /// the service, matching the dual-Arc pattern.
    pub fn register_with_source<T>(
        &self,
        source: Source,
        provider_id: ProviderId,
        payload: Arc<dyn Any + Send + Sync>,
    ) -> Result<(), ServiceRegistryError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.peer_source.register::<T>(source, provider_id, payload)
    }

    /// Look up a service by capsule id and capability type.
    ///
    /// Returns `Ok(Arc<T>)` if found, or
    /// `Err(ServiceRegistryError::SourceNotFound)` otherwise.
    pub fn get_by_capsule<T>(
        &self,
        capsule_id: &CapsuleId,
    ) -> Result<Arc<T>, ServiceRegistryError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.peer_source.get_by_capsule::<T>(capsule_id)
    }

    /// Look up a service by stream id and capability type.
    pub fn get_by_stream<T>(
        &self,
        stream_id: &StreamId,
    ) -> Result<Arc<T>, ServiceRegistryError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.peer_source.get_by_stream::<T>(stream_id)
    }

    /// Evict all services associated with a capsule.
    pub fn evict_capsule(&self, capsule_id: &CapsuleId) {
        self.peer_source.evict_capsule(capsule_id);
    }

    /// Evict all services associated with a stream.
    pub fn evict_stream(&self, stream_id: &StreamId) {
        self.peer_source.evict_stream(stream_id);
    }

    /// String-based resolution for diagnostics/introspection.
    pub fn resolve(&self, name: &str) -> Option<Arc<dyn Service>> {
        let index = self.name_index.read();
        index.get(name).cloned()
    }

    /// Observe a service's lifecycle state.
    pub fn state(&self, _key: &ServiceKey) -> Option<ServiceState> {
        // TODO: track state per entry
        None
    }

    /// Snapshot all stateful services.
    pub async fn snapshot_all(&self) -> HashMap<String, serde_json::Value> {
        // TODO: iterate stateful services
        HashMap::new()
    }

    /// Restore all stateful services from a snapshot.
    pub async fn restore_all(
        &self,
        _snapshot: HashMap<String, serde_json::Value>,
    ) -> Result<(), ServiceError> {
        // TODO: restore stateful services
        Ok(())
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal service for TypeId validation testing.
    struct TestService;

    impl Service for TestService {
        fn name(&self) -> &str {
            "test-service"
        }
    }

    #[test]
    fn register_and_resolve_by_name() {
        let registry = ServiceRegistry::new();
        let desc = ServiceDescriptor {
            name: "test-service".to_string(),
            category: crate::traits::ServiceCategory::Custom,
            version: crate::traits::SemverVersion::new(0, 1, 0),
        };
        let svc = Arc::new(TestService);
        let result = registry.register_by_name(svc, desc);
        assert!(result.is_ok());

        let resolved = registry.resolve("test-service");
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().name(), "test-service");
    }

    #[test]
    fn resolve_unknown_returns_none() {
        let registry = ServiceRegistry::new();
        assert!(registry.resolve("nonexistent").is_none());
    }

    #[test]
    fn register_typed_typeid_validation() {
        let registry = ServiceRegistry::new();
        let desc = ServiceDescriptor {
            name: "typed-service".to_string(),
            category: crate::traits::ServiceCategory::Custom,
            version: crate::traits::SemverVersion::new(0, 1, 0),
        };

        // Register with correct TypeId
        let svc: Arc<dyn Service> = Arc::new(TestService);
        let typed: Arc<dyn Any + Send + Sync> = Arc::new(svc.clone());
        let type_id = (*typed).type_id();

        let result = registry.register_typed(typed, type_id, svc, desc);
        assert!(result.is_ok());

        // Can resolve by name
        let resolved = registry.resolve("typed-service");
        assert!(resolved.is_some());
    }

    #[test]
    fn register_provider_bulk() {
        use async_trait::async_trait;

        struct TestProvider;

        #[async_trait]
        impl crate::provider::ServiceProvider for TestProvider {
            fn id(&self) -> &str {
                "test-provider"
            }

            async fn list_services(&self) -> Vec<ServiceDescriptor> {
                vec![ServiceDescriptor {
                    name: "svc-a".to_string(),
                    category: crate::traits::ServiceCategory::Custom,
                    version: crate::traits::SemverVersion::new(0, 1, 0),
                }]
            }

            async fn get_service(
                &self,
                name: &str,
            ) -> Option<Arc<dyn Service>> {
                if name == "svc-a" {
                    Some(Arc::new(TestService))
                } else {
                    None
                }
            }

            fn dependencies(&self) -> &'static [ServiceKey] {
                &[]
            }
        }

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let registry = ServiceRegistry::new();
            let provider = Arc::new(TestProvider);
            let tokens = registry.register_provider(provider).await.unwrap();
            assert_eq!(tokens.len(), 1);
            assert!(registry.resolve("svc-a").is_some());
        });
    }
}
