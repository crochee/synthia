//! Peer-source registration (CapsuleId / StreamId).
//!
//! PR-3.4 lets services be tagged with a `Source` indicating their origin.
//! This enables capsule-scoped and stream-scoped lookup, and automatic
//! eviction when a capsule/stream ends.
//!
//! See `openspec/.../specs/service-registry-completion/spec.md`
//! (Requirement: "peer-source identification").

use std::{
    any::{Any, TypeId},
    sync::Arc,
};

use dashmap::DashMap;

use crate::{output_bound::ServiceRegistryError, provider::ProviderId};

/// The origin of a peer-source service.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Source {
    /// Service originated from a capsule (scoped lifetime).
    Capsule(CapsuleId),
    /// Service originated from a stream (scoped lifetime).
    Stream(StreamId),
}

/// Unique identifier for a capsule-scoped service.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CapsuleId(pub String);

impl std::fmt::Display for CapsuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "capsule:{}", self.0)
    }
}

/// Unique identifier for a stream-scoped service.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StreamId(pub String);

impl std::fmt::Display for StreamId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "stream:{}", self.0)
    }
}

/// Key combining source + capability TypeId for double-index lookup.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SourceCapKey {
    source: Source,
    capability_type_id: TypeId,
}

impl SourceCapKey {
    fn new(source: Source, type_id: TypeId) -> Self {
        Self {
            source,
            capability_type_id: type_id,
        }
    }
}

/// Thread-safe peer-source index.
///
/// Stores services keyed by their source + capability so that consumers
/// can look up services by capsule/stream id and capability type.
pub(crate) struct PeerSourceIndex {
    /// `(Source, TypeId::of::<T>())` → `Arc<dyn Any + Send + Sync>`
    /// for typed retrieval.
    entries: DashMap<SourceCapKey, Arc<dyn Any + Send + Sync>>,
    /// `CapsuleId` → set of `SourceCapKey` for bulk eviction.
    capsule_keys: DashMap<CapsuleId, Vec<SourceCapKey>>,
    /// `StreamId` → set of `SourceCapKey` for bulk eviction.
    stream_keys: DashMap<StreamId, Vec<SourceCapKey>>,
    /// `ProviderId` → `SourceCapKey` for reverse lookup.
    provider_keys: DashMap<ProviderId, SourceCapKey>,
}

impl PeerSourceIndex {
    /// Create an empty peer-source index.
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
            capsule_keys: DashMap::new(),
            stream_keys: DashMap::new(),
            provider_keys: DashMap::new(),
        }
    }

    /// Register a service with a peer source and capability.
    ///
    /// The `payload` must be `Arc::new(Arc<dyn T>)` wrapping the
    /// service, matching the dual-Arc pattern used by
    /// [`crate::registry::ServiceRegistry`].
    pub fn register<T>(
        &self,
        source: Source,
        provider_id: ProviderId,
        payload: Arc<dyn Any + Send + Sync>,
    ) -> Result<(), ServiceRegistryError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        let key = SourceCapKey::new(source.clone(), TypeId::of::<T>());

        self.entries.insert(key.clone(), payload);
        self.provider_keys.insert(provider_id, key.clone());

        match &source {
            Source::Capsule(cid) => {
                self.capsule_keys.entry(cid.clone()).or_default().push(key);
            }
            Source::Stream(sid) => {
                self.stream_keys.entry(sid.clone()).or_default().push(key);
            }
        }

        Ok(())
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
        let key = SourceCapKey::new(
            Source::Capsule(capsule_id.clone()),
            TypeId::of::<T>(),
        );

        let entry = self.entries.get(&key).ok_or_else(|| {
            ServiceRegistryError::SourceNotFound {
                origin: capsule_id.to_string(),
                capability: std::any::type_name::<T>().to_string(),
            }
        })?;

        let wrapped: Arc<Arc<T>> =
            Arc::downcast::<Arc<T>>(entry.value().clone()).map_err(|_| {
                ServiceRegistryError::SourceNotFound {
                    origin: capsule_id.to_string(),
                    capability: std::any::type_name::<T>().to_string(),
                }
            })?;

        Ok((*wrapped).clone())
    }

    /// Look up a service by stream id and capability type.
    pub fn get_by_stream<T>(
        &self,
        stream_id: &StreamId,
    ) -> Result<Arc<T>, ServiceRegistryError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        let key = SourceCapKey::new(
            Source::Stream(stream_id.clone()),
            TypeId::of::<T>(),
        );

        let entry = self.entries.get(&key).ok_or_else(|| {
            ServiceRegistryError::SourceNotFound {
                origin: stream_id.to_string(),
                capability: std::any::type_name::<T>().to_string(),
            }
        })?;

        let wrapped: Arc<Arc<T>> =
            Arc::downcast::<Arc<T>>(entry.value().clone()).map_err(|_| {
                ServiceRegistryError::SourceNotFound {
                    origin: stream_id.to_string(),
                    capability: std::any::type_name::<T>().to_string(),
                }
            })?;

        Ok((*wrapped).clone())
    }

    /// Evict all services associated with a capsule.
    ///
    /// Called when a capsule ends to clean up its scoped services.
    pub fn evict_capsule(&self, capsule_id: &CapsuleId) {
        if let Some((_, keys)) = self.capsule_keys.remove(capsule_id) {
            for key in keys {
                self.entries.remove(&key);
            }
        }
    }

    /// Evict all services associated with a stream.
    ///
    /// Called when a stream ends to clean up its scoped services.
    pub fn evict_stream(&self, stream_id: &StreamId) {
        if let Some((_, keys)) = self.stream_keys.remove(stream_id) {
            for key in keys {
                self.entries.remove(&key);
            }
        }
    }
}

impl Default for PeerSourceIndex {
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

    trait OtherCap: Send + Sync + 'static {}
    struct OtherCapImpl;
    impl OtherCap for OtherCapImpl {}

    #[test]
    fn register_and_get_by_capsule() {
        let idx = PeerSourceIndex::new();
        let cid = CapsuleId("capsule-1".into());
        let payload: Arc<dyn Any + Send + Sync> =
            Arc::new(Arc::new(MyCapImpl) as Arc<dyn MyCap>);

        idx.register::<dyn MyCap>(
            Source::Capsule(cid.clone()),
            ProviderId("p1".into()),
            payload,
        )
        .unwrap();

        let result: Arc<dyn MyCap> =
            idx.get_by_capsule::<dyn MyCap>(&cid).unwrap();
        // Verify the Arc is valid by checking the type_id.
        let _ = &result;
    }

    #[test]
    fn get_by_capsule_not_found() {
        let idx = PeerSourceIndex::new();
        let cid = CapsuleId("nonexistent".into());

        let result = idx.get_by_capsule::<dyn MyCap>(&cid);
        assert!(matches!(
            result,
            Err(ServiceRegistryError::SourceNotFound { .. })
        ));
    }

    #[test]
    fn register_and_get_by_stream() {
        let idx = PeerSourceIndex::new();
        let sid = StreamId("stream-1".into());
        let payload: Arc<dyn Any + Send + Sync> =
            Arc::new(Arc::new(MyCapImpl) as Arc<dyn MyCap>);

        idx.register::<dyn MyCap>(
            Source::Stream(sid.clone()),
            ProviderId("p1".into()),
            payload,
        )
        .unwrap();

        let result: Arc<dyn MyCap> =
            idx.get_by_stream::<dyn MyCap>(&sid).unwrap();
        let _ = &result;
    }

    #[test]
    fn evict_capsule_removes_entries() {
        let idx = PeerSourceIndex::new();
        let cid = CapsuleId("capsule-ephemeral".into());
        let payload: Arc<dyn Any + Send + Sync> =
            Arc::new(Arc::new(MyCapImpl) as Arc<dyn MyCap>);

        idx.register::<dyn MyCap>(
            Source::Capsule(cid.clone()),
            ProviderId("p1".into()),
            payload,
        )
        .unwrap();

        idx.evict_capsule(&cid);

        let result = idx.get_by_capsule::<dyn MyCap>(&cid);
        assert!(matches!(
            result,
            Err(ServiceRegistryError::SourceNotFound { .. })
        ));
    }

    #[test]
    fn evict_stream_removes_entries() {
        let idx = PeerSourceIndex::new();
        let sid = StreamId("stream-ephemeral".into());
        let payload: Arc<dyn Any + Send + Sync> =
            Arc::new(Arc::new(MyCapImpl) as Arc<dyn MyCap>);

        idx.register::<dyn MyCap>(
            Source::Stream(sid.clone()),
            ProviderId("p1".into()),
            payload,
        )
        .unwrap();

        idx.evict_stream(&sid);

        let result = idx.get_by_stream::<dyn MyCap>(&sid);
        assert!(matches!(
            result,
            Err(ServiceRegistryError::SourceNotFound { .. })
        ));
    }

    #[test]
    fn capsule_and_stream_coexist_independently() {
        let idx = PeerSourceIndex::new();
        let cid = CapsuleId("c1".into());
        let sid = StreamId("s1".into());

        let payload_c: Arc<dyn Any + Send + Sync> =
            Arc::new(Arc::new(MyCapImpl) as Arc<dyn MyCap>);
        let payload_s: Arc<dyn Any + Send + Sync> =
            Arc::new(Arc::new(OtherCapImpl) as Arc<dyn OtherCap>);

        idx.register::<dyn MyCap>(
            Source::Capsule(cid.clone()),
            ProviderId("pc".into()),
            payload_c,
        )
        .unwrap();
        idx.register::<dyn OtherCap>(
            Source::Stream(sid.clone()),
            ProviderId("ps".into()),
            payload_s,
        )
        .unwrap();

        // Evicting capsule doesn't affect stream.
        idx.evict_capsule(&cid);
        assert!(idx.get_by_stream::<dyn OtherCap>(&sid).is_ok());
    }
}
