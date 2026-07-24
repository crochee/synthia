//! Stateful `ModelRouter`: provider registration, routing rules,
//! fallback chains, and selection methods.

mod availability;
mod config;
mod registration;
mod selection;
mod types;

pub use types::{FallbackChainConfig, ModelRouter};

impl Default for ModelRouter {
    fn default() -> Self {
        Self::new()
    }
}
