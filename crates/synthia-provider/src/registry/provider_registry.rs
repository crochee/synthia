use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use synthia_core::{
    Error,
    registry::Registry,
    tool::extension_registry::ProviderStore,
};

use super::types::{ProviderFilter, ProviderInfo};
use crate::{
    router::{RoutingConfig, RoutingContext, RoutingRule, RuleEvaluator},
    traits::ModelProvider,
};

pub struct ProviderRegistry {
    providers: RwLock<HashMap<String, Arc<dyn ModelProvider>>>,
    routing_rules: RwLock<Vec<RoutingRule>>,
    fallback_chain: RwLock<HashMap<String, Vec<String>>>,
    active_provider: RwLock<Option<String>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
            routing_rules: RwLock::new(Vec::new()),
            fallback_chain: RwLock::new(HashMap::new()),
            active_provider: RwLock::new(None),
        }
    }

    pub fn register_provider(&self, provider: Box<dyn ModelProvider>) {
        let name = provider.name().to_string();
        if let Ok(mut providers) = self.providers.write() {
            providers.insert(name, Arc::from(provider));
        }
    }

    pub fn select(
        &self,
        context: &RoutingContext,
    ) -> Result<Arc<dyn ModelProvider>, Error> {
        if let Ok(active) = self.active_provider.read()
            && let Some(ref name) = *active
        {
            let providers = self.providers.read().map_err(|_| {
                Error::Internal("Failed to acquire read lock".to_string())
            })?;
            if let Some(provider) = providers.get(name) {
                return Ok(Arc::clone(provider));
            }
            return Err(Error::NotFound(name.clone()));
        }

        let rules = self.routing_rules.read().map_err(|_| {
            Error::Internal("Failed to acquire read lock".to_string())
        })?;

        if let Ok(rule) = RuleEvaluator::evaluate(&rules, context) {
            let provider_name = &rule.provider_name;
            let providers = self.providers.read().map_err(|_| {
                Error::Internal("Failed to acquire read lock".to_string())
            })?;
            if let Some(provider) = providers.get(provider_name) {
                return Ok(Arc::clone(provider));
            }

            drop(providers);
            if let Ok(fallbacks) = self.fallback_chain.read()
                && let Some(chain) = fallbacks.get(&rule.model_name)
            {
                let providers = self.providers.read().map_err(|_| {
                    Error::Internal("Failed to acquire read lock".to_string())
                })?;
                for fallback_name in chain {
                    if let Some(provider) = providers.get(fallback_name) {
                        return Ok(Arc::clone(provider));
                    }
                }
            }
        }

        let providers = self.providers.read().map_err(|_| {
            Error::Internal("Failed to acquire read lock".to_string())
        })?;
        providers.values().next().map(Arc::clone).ok_or_else(|| {
            Error::NotFound("No providers registered".to_string())
        })
    }

    pub fn set_active_provider(&self, name: &str) -> Result<(), Error> {
        let providers = self.providers.read().map_err(|_| {
            Error::Internal("Failed to acquire read lock".to_string())
        })?;
        if !providers.contains_key(name) {
            return Err(Error::NotFound(name.to_string()));
        }
        drop(providers);

        let mut active = self.active_provider.write().map_err(|_| {
            Error::Internal("Failed to acquire write lock".to_string())
        })?;
        *active = Some(name.to_string());
        Ok(())
    }

    pub fn clear_active_provider(&self) {
        if let Ok(mut active) = self.active_provider.write() {
            *active = None;
        }
    }

    pub fn load_routing_config(&self, config: &RoutingConfig) {
        if let Ok(mut rules) = self.routing_rules.write() {
            *rules = config
                .routes
                .values()
                .map(|entry| RoutingRule {
                    condition: crate::router::RoutingCondition::Complexity(
                        crate::router::ComplexityLevel::Simple,
                    ),
                    provider_name: entry.provider.clone(),
                    model_name: entry.model.clone(),
                    priority: 1,
                })
                .collect();
        }

        if let Ok(mut fallbacks) = self.fallback_chain.write() {
            for entry in config.routes.values() {
                if let Some(ref fallback) = entry.fallback {
                    fallbacks
                        .entry(entry.model.clone())
                        .or_default()
                        .push(fallback.clone());
                }
            }
        }
    }

    pub fn contains(&self, name: &str) -> bool {
        self.providers
            .read()
            .map(|p| p.contains_key(name))
            .unwrap_or(false)
    }

    pub fn len(&self) -> usize {
        self.providers.read().map(|p| p.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.providers.read().map(|p| p.is_empty()).unwrap_or(true)
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Registry<ProviderInfo> for ProviderRegistry {
    type Filter = ProviderFilter;

    async fn register(
        &self,
        _item: ProviderInfo,
    ) -> Result<ProviderInfo, Error> {
        Err(Error::Internal(
            "Provider registration requires Box<dyn ModelProvider>, use register_provider() instead"
                .to_string(),
        ))
    }

    async fn unregister(&self, name: &str) -> Result<(), Error> {
        let mut providers = self.providers.write().map_err(|_| {
            Error::Internal("Failed to acquire write lock".to_string())
        })?;
        let removed = providers.remove(name);
        drop(providers);
        match removed {
            Some(_) => Ok(()),
            None => Err(Error::NotFound(name.to_string())),
        }
    }

    async fn get(&self, name: &str) -> Result<Option<ProviderInfo>, Error> {
        let providers = self.providers.read().map_err(|_| {
            Error::Internal("Failed to acquire read lock".to_string())
        })?;
        Ok(providers.get(name).map(|p| ProviderInfo {
            name: p.name().to_string(),
            description: "Model provider".to_string(),
        }))
    }

    async fn list(
        &self,
        filter: Option<Self::Filter>,
    ) -> Result<Vec<ProviderInfo>, Error> {
        let filter = filter.unwrap_or_default();
        let providers = self.providers.read().map_err(|_| {
            Error::Internal("Failed to acquire read lock".to_string())
        })?;
        let result: Vec<ProviderInfo> = providers
            .values()
            .filter(|p| {
                let info = ProviderInfo {
                    name: p.name().to_string(),
                    description: "Model provider".to_string(),
                };
                filter.accepts(&info)
            })
            .map(|p| ProviderInfo {
                name: p.name().to_string(),
                description: "Model provider".to_string(),
            })
            .collect();
        Ok(result)
    }
}

impl ProviderStore for ProviderRegistry {
    fn provider_count(&self) -> usize {
        self.len()
    }

    fn contains_provider(&self, name: &str) -> bool {
        self.contains(name)
    }

    fn is_empty(&self) -> bool {
        ProviderRegistry::is_empty(self)
    }
}
