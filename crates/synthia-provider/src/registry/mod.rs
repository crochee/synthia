mod provider_registry;
mod types;

#[cfg(test)]
mod tests;

pub use provider_registry::ProviderRegistry;
pub use types::{ProviderFilter, ProviderInfo};

pub mod v2;
pub use v2::{
    ProviderRegistry as ProviderRegistryV2,
    RegisteredProvider,
    RegistryError,
    SourceId,
};

pub mod v2_events;
pub use v2_events::ProviderEvent;
