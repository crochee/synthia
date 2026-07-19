//! Typed `Capability<T>` contract for explicit service capability declaration.
//!
//! PR-3.2 introduces `Capability<T>` — a marker that lets services declare
//! what typed interface they expose. Consumers query the registry for all
//! providers of a given capability via `capabilities_provided::<T>()`.
//!
//! See `openspec/.../specs/service-registry-completion/spec.md`
//! (Requirement: "Capability typed contract").

use std::{
    any::{Any, TypeId},
    sync::Arc,
};

use dashmap::DashMap;

use crate::{output_bound::ServiceRegistryError, provider::ProviderId};

/// A typed capability marker.
///
/// `Capability<T>` is a zero-sized type parameterised by the trait that
/// the capability exposes. It is never instantiated at runtime — it only
/// carries the `TypeId` of `T` for registry indexing.
///
/// # Example
///
/// ```ignore
/// registry.register_with_capability(svc, Capability::of::<dyn MyTrait>(), provider_id)?;
/// let providers = registry.capabilities_provided::<dyn MyTrait>();
/// ```
pub struct Capability<T: ?Sized + 'static> {
    _marker: std::marker::PhantomData<T>,
}

impl<T: ?Sized + 'static> Capability<T> {
    /// Obtain the `Capability` marker for trait `T`.
    ///
    /// This is the only way to construct a `Capability`; the type is
    /// zero-sized and never instantiated.
    pub const fn of() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }

    /// The `TypeId` of `T`, used as the registry key.
    pub fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    /// The `type_name` of `T`, for diagnostics.
    pub fn type_name(&self) -> &'static str {
        std::any::type_name::<T>()
    }
}

/// Thread-safe storage for capability → provider mappings.
///
/// Each `TypeId` (representing a trait) maps to a list of `ProviderId`s
/// that declared they provide that capability.
pub(crate) struct CapabilityIndex {
    /// `TypeId::of::<T>()` → `Vec<ProviderId>` listing providers.
    index: DashMap<TypeId, Vec<ProviderId>>,
    /// `TypeId::of::<T>()` → `Arc<dyn Any + Send + Sync>` for typed
    /// downcast verification on register.
    payloads: DashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl CapabilityIndex {
    /// Create an empty capability index.
    pub fn new() -> Self {
        Self {
            index: DashMap::new(),
            payloads: DashMap::new(),
        }
    }

    /// Register a service as providing capability `T`.
    ///
    /// The `payload` is stored alongside the `ProviderId` so that
    /// `register_with_capability` can verify the actual type matches
    /// the declared capability (PR-3.2 spec scenario "capability mismatch
    /// on register").
    pub fn register<T>(
        &self,
        provider_id: ProviderId,
        payload: Arc<dyn Any + Send + Sync>,
        cap: &Capability<T>,
    ) -> Result<(), ServiceRegistryError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        let type_id = cap.type_id();

        // Verify that the payload actually implements T by checking
        // its TypeId matches what Arc<T> would have.
        let actual_type_id = (*payload).type_id();
        let expected_type_id = TypeId::of::<Arc<T>>();
        if actual_type_id != expected_type_id {
            return Err(ServiceRegistryError::CapabilityMismatch {
                expected: cap.type_name().to_string(),
                found: std::any::type_name_of_val(&*payload).to_string(),
            });
        }

        self.payloads.insert(type_id, payload);

        let mut entry = self.index.entry(type_id).or_default();
        if !entry.contains(&provider_id) {
            entry.push(provider_id);
        }

        Ok(())
    }

    /// Query all providers of capability `T`.
    pub fn providers<T>(&self, cap: &Capability<T>) -> Vec<ProviderId>
    where
        T: ?Sized + 'static,
    {
        self.index
            .get(&cap.type_id())
            .map(|r| r.value().clone())
            .unwrap_or_default()
    }
}

impl Default for CapabilityIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    trait MyCap: Send + Sync + 'static {}
    struct MyCapImpl;
    impl MyCap for MyCapImpl {}

    #[test]
    fn capability_of_returns_consistent_type_id() {
        let cap = Capability::<dyn MyCap>::of();
        assert_eq!(cap.type_id(), TypeId::of::<dyn MyCap>());
    }

    #[test]
    fn capability_index_register_and_query() {
        let idx = CapabilityIndex::new();
        let provider_id = ProviderId("test-provider".into());
        let payload: Arc<dyn Any + Send + Sync> =
            Arc::new(Arc::new(MyCapImpl) as Arc<dyn MyCap>);
        let cap = Capability::<dyn MyCap>::of();

        idx.register(provider_id.clone(), payload, &cap).unwrap();

        let providers = idx.providers(&cap);
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0], provider_id);
    }

    #[test]
    fn capability_mismatch_rejected() {
        let idx = CapabilityIndex::new();
        let provider_id = ProviderId("mismatch".into());
        // Register a plain String as the payload, which does NOT implement
        // Arc<dyn MyCap>.
        let payload: Arc<dyn Any + Send + Sync> =
            Arc::new("not my cap".to_string());
        let cap = Capability::<dyn MyCap>::of();

        let result = idx.register(provider_id, payload, &cap);
        assert!(matches!(
            result,
            Err(ServiceRegistryError::CapabilityMismatch { .. })
        ));
    }

    #[test]
    fn duplicate_provider_id_deduped() {
        let idx = CapabilityIndex::new();
        let provider_id = ProviderId("dup".into());
        let payload: Arc<dyn Any + Send + Sync> =
            Arc::new(Arc::new(MyCapImpl) as Arc<dyn MyCap>);
        let cap = Capability::<dyn MyCap>::of();

        idx.register(provider_id.clone(), payload.clone(), &cap)
            .unwrap();
        idx.register(provider_id.clone(), payload, &cap).unwrap();

        let providers = idx.providers(&cap);
        assert_eq!(providers.len(), 1);
    }
}
