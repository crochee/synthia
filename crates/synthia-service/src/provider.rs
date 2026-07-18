//! ServiceProvider — the registration contract for services.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::traits::{SemverVersion, Service, ServiceCategory, ServiceKey};

/// Source of services. Multiple providers can be registered,
/// each contributing a disjoint or overlapping set of services.
#[async_trait]
pub trait ServiceProvider: Send + Sync + 'static {
    /// Stable provider id (for diagnostics, hot-reload).
    fn id(&self) -> &str;

    /// Advertise all services this provider exposes.
    async fn list_services(&self) -> Vec<ServiceDescriptor>;

    /// Resolve a service by name. Returns None if not provided.
    async fn get_service(
        &self,
        name: &str,
    ) -> Option<std::sync::Arc<dyn Service>>;

    /// Dependency declaration (for ordering init).
    fn dependencies(&self) -> &'static [ServiceKey] {
        &[]
    }
}

/// Metadata for a service advertisement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDescriptor {
    pub name: String,
    pub version: SemverVersion,
    pub category: ServiceCategory,
}

/// Unique provider identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProviderId(pub String);

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Token returned by successful registration. Used for unregistration.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RegistrationToken(pub u64);

/// Registration error variants.
#[derive(Debug, thiserror::Error)]
pub enum RegistrationError {
    #[error("duplicate service name: {0}")]
    DuplicateName(String),
    #[error("core service name taken: {name}")]
    CoreNameTaken { name: String },
    #[error("registration failed: {0}")]
    Failed(String),
}
