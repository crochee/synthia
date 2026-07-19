//! `OutputBoundService` — typed-view trait (PR-3.1).
//!
//! `OutputBoundService` lets a concrete `Service` advertise a *dyn-compatible*
//! capability that the registry can hand out without exposing the concrete
//! type. This is the same pattern [`crate::traits::ErasedStatefulService`]
//! uses to expose `StatefulService<S>` for storage as `serde_json::Value`.
//!
//! See `openspec/changes/2026-07-18-synthia-top5-borrow-integration/specs/service-registry-completion/spec.md`
//! (Requirement: "OutputBound::Service trait"). The acceptance criterion
//! is "type-system test (no runtime panic for non-Send types) passes"
//! per `tasks.md` Task 3.1.

use std::sync::Arc;

/// A `Service` that exposes a typed capability through [`Self::Service`].
///
/// Implementors pick the dyn-compatible view that downstream consumers
/// query the registry with via
/// [`ServiceRegistry::bound_service`](crate::registry::ServiceRegistry::bound_service).
pub trait OutputBoundService: Send + Sync + 'static {
    /// The capability view handed back to callers of `bound_service::<Self::Service>()`.
    type Service: ?Sized + Send + Sync + 'static;

    /// Project `self` to the bound view. Each call MUST return an
    /// `Arc` that aliases the underlying state for the lifetime of
    /// any consumer — registry callers rely on the returned `Arc`
    /// remaining valid as long as the registry still owns the
    /// `Service` instance.
    fn as_bound(&self) -> Arc<Self::Service>;
}

/// Errors that surface from the [`OutputBoundService`](self) lookup path.
///
/// PR-3.1 carries the `NotBound` variant. PR-3.2 adds `CapabilityMismatch`,
/// PR-3.3 adds `Cycle`, and PR-3.4 adds `SourceNotFound` — each landing
/// with its respective registry entry point.
#[derive(Debug, thiserror::Error)]
pub enum ServiceRegistryError {
    /// No service is bound for the requested capability. The bound
    /// `&'static str` is the `type_name::<T>()` of the missing view.
    #[error("no service bound for capability {0}")]
    NotBound(&'static str),

    /// The service's actual type does not match the declared capability.
    #[error("capability mismatch: expected {expected}, found {found}")]
    CapabilityMismatch { expected: String, found: String },

    /// Adding the edge would introduce a cycle in the dependency graph.
    #[error("dependency cycle detected: {path}")]
    Cycle { path: String },

    /// No service found for the given source and capability.
    #[error("source not found: source={origin}, capability={capability}")]
    SourceNotFound { origin: String, capability: String },
}
