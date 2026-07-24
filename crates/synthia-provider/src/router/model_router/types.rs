use std::collections::HashMap;

use super::super::RoutingConfig;

/// Primary model + ordered list of backup models loaded from TOML
/// (or registered programmatically).
#[derive(Debug, Clone)]
pub struct FallbackChainConfig {
    pub primary: String,
    pub fallbacks: Vec<String>,
}

pub struct ModelRouter {
    pub(super) providers: HashMap<String, crate::ModelConfig>,
    pub(super) routing_rules: Vec<super::super::config::RoutingRule>,
    pub(super) fallback_chains: HashMap<String, Vec<String>>,
    pub(super) cost_per_request: HashMap<String, f64>,
    pub(super) provider_registry: Option<crate::ProviderRegistry>,
    pub(in super::super) routing_config: RoutingConfig,
}
