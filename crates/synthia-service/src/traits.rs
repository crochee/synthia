//! Service trait — system-internal capabilities NOT exposed to LLM.
//!
//! Every system capability (Session, Memory, Permission, Hook, etc.)
//! implements this trait. Services are registered via [`ServiceProvider`]
//! and resolved from [`ServiceRegistry`].

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// System-internal service. NOT exposed to LLM. Used by:
/// - Loop (consumes services via `OperationContext`)
/// - Tools (consume services via `CapabilityBroker`)
/// - Extensions (register services)
#[async_trait]
pub trait Service: Send + Sync + 'static {
    /// Human-readable name for diagnostics. NOT used as a registry key —
    /// typed resolution uses `TypeId::of::<Arc<dyn SubTrait>>()`.
    fn name(&self) -> &str;

    /// Semantic version of this service implementation.
    fn version(&self) -> SemverVersion {
        SemverVersion::new(0, 1, 0)
    }

    /// Initialize the service. Called once after registration.
    async fn init(&self, ctx: &ServiceInitContext) -> Result<(), ServiceError> {
        let _ = ctx;
        Ok(())
    }

    /// Gracefully shut down the service.
    async fn shutdown(&self) -> Result<(), ServiceError> {
        Ok(())
    }
}

/// Marker trait: services that hold serializable state.
///
/// This trait is intentionally NOT dyn-compatible (it has an associated
/// type). Use [`ErasedStatefulService`] for dyn-compatible snapshot/restore.
#[async_trait]
pub trait StatefulService: Service {
    type State: Send + Sync + 'static;

    /// Capture current state for persistence.
    async fn snapshot(&self) -> Result<Self::State, ServiceError>;

    /// Restore from a previously captured state.
    async fn restore(&self, state: Self::State) -> Result<(), ServiceError>;
}

/// Dyn-compatible view of [`StatefulService`] for registry storage.
///
/// Returns `serde_json::Value` instead of an associated type,
/// so it can be stored as `Arc<dyn ErasedStatefulService>`.
#[async_trait]
pub trait ErasedStatefulService: Service {
    async fn snapshot_json(&self) -> Result<serde_json::Value, ServiceError>;
    async fn restore_json(
        &self,
        state: serde_json::Value,
    ) -> Result<(), ServiceError>;
}

/// Blanket impl: every `StatefulService` with serde-compatible `State`
/// is automatically an `ErasedStatefulService`.
#[async_trait]
impl<T> ErasedStatefulService for T
where
    T: StatefulService + Send + Sync,
    T::State: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    async fn snapshot_json(&self) -> Result<serde_json::Value, ServiceError> {
        let typed = self.snapshot().await?;
        serde_json::to_value(typed).map_err(ServiceError::Serialization)
    }

    async fn restore_json(
        &self,
        state: serde_json::Value,
    ) -> Result<(), ServiceError> {
        let typed: T::State = serde_json::from_value(state)
            .map_err(ServiceError::Deserialization)?;
        self.restore(typed).await
    }
}

/// Context passed to `Service::init`.
#[derive(Debug, Clone)]
pub struct ServiceInitContext {
    /// Workspace root directory.
    pub workspace_root: std::path::PathBuf,
}

/// Semantic version (major.minor.patch).
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct SemverVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SemverVersion {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl std::fmt::Display for SemverVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Service lifecycle state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceState {
    Constructed,
    Initializing,
    Initialized,
    Running,
    ShuttingDown,
    Dropped,
}

/// Service category for classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServiceCategory {
    Session,
    Memory,
    Permission,
    Guardian,
    Hook,
    Extension,
    Configuration,
    Telemetry,
    Skill,
    Command,
    Task,
    Scheduler,
    Goal,
    Custom,
}

/// Typed service error.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("deserialization failed: {0}")]
    Deserialization(serde_json::Error),
    #[error("init failed: {0}")]
    InitFailed(String),
    #[error("dependency missing: {0:?}")]
    DependencyMissing(ServiceKey),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("state invalid: expected {expected:?}, got {actual:?}")]
    StateInvalid {
        expected: ServiceState,
        actual: ServiceState,
    },
    #[error("capability denied: need {need} for service {service}")]
    CapabilityDenied {
        service: ServiceKey,
        need: &'static str,
    },
    #[error("already running")]
    AlreadyRunning,
    #[error("no such run")]
    NoSuchRun,
}

/// Service key for typed lookups and dependency declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServiceKey {
    type_id: std::any::TypeId,
    name: &'static str,
}

impl ServiceKey {
    /// Create a key for a specific service subtrait.
    pub fn of<T: 'static + ?Sized + Service>() -> Self {
        Self {
            type_id: std::any::TypeId::of::<T>(),
            name: std::any::type_name::<T>(),
        }
    }

    pub fn type_id(&self) -> std::any::TypeId {
        self.type_id
    }

    pub fn name(&self) -> &'static str {
        self.name
    }
}

impl std::fmt::Display for ServiceKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
