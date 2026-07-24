use std::collections::HashMap;

use super::ModelRouter;

impl ModelRouter {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            routing_rules: Vec::new(),
            fallback_chains: HashMap::new(),
            cost_per_request: HashMap::new(),
            provider_registry: None,
            routing_config: super::super::RoutingConfig::default(),
        }
    }

    pub fn set_provider_registry(&mut self, registry: crate::ProviderRegistry) {
        self.provider_registry = Some(registry);
    }

    pub fn register_provider(
        &mut self,
        name: String,
        config: crate::ModelConfig,
    ) {
        self.providers.insert(name.clone(), config);
    }

    pub fn set_model_cost(&mut self, name: String, cost: f64) {
        self.cost_per_request.insert(name, cost);
    }

    pub fn add_rule(&mut self, rule: super::super::config::RoutingRule) {
        self.routing_rules.push(rule);
    }

    pub fn set_fallback_chain(
        &mut self,
        primary: String,
        fallbacks: Vec<String>,
    ) {
        self.fallback_chains.insert(primary, fallbacks);
    }
}
