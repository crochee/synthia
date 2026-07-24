//! Model service for model provider management logic

use std::sync::Arc;

use super::types::{
    AddModelProviderRequest,
    ModelInfo,
    ProviderInfo,
    UpdateModelRequest,
};
use crate::{AppState, error::ServerError};

pub struct ModelService {
    state: Arc<AppState>,
}

impl ModelService {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    pub async fn list_providers(&self) -> Vec<ProviderInfo> {
        let config = self.state.config.read().await;
        config
            .providers
            .iter()
            .map(|(name, provider)| ProviderInfo {
                name: name.clone(),
                api_key: provider.api_key.clone(),
                base_url: provider.base_url.clone(),
                models: provider.models.clone(),
            })
            .collect()
    }

    pub async fn get_model(
        &self,
        provider_name: &str,
        model_name: &str,
    ) -> Result<ModelInfo, ServerError> {
        let config = self.state.config.read().await;

        let provider = config
            .providers
            .get(provider_name)
            .ok_or_else(|| ServerError::not_found("Provider", provider_name))?;

        let model = provider
            .models
            .iter()
            .find(|m| m.name == model_name)
            .ok_or_else(|| {
                ServerError::not_found(
                    format!("Model in provider '{}'", provider_name),
                    model_name,
                )
            })?;

        Ok(ModelInfo {
            provider: provider_name.to_string(),
            name: model.name.clone(),
            description: model.description.clone(),
            context_window: model.context_window,
            temperature: model.temperature,
            max_tokens: model.max_tokens,
            api_key: provider.api_key.clone(),
            base_url: provider.base_url.clone(),
        })
    }

    pub async fn add_provider(
        &self,
        request: AddModelProviderRequest,
    ) -> Result<ProviderInfo, ServerError> {
        if request.name.is_empty() {
            return Err(ServerError::missing_field("name"));
        }

        let mut config = self.state.config.write().await;

        if config.providers.contains_key(&request.name) {
            return Err(ServerError::already_exists("Provider", &request.name));
        }

        let provider_config = crate::config::ProviderConfig {
            api_key: request.api_key.clone(),
            base_url: request.base_url.clone(),
            models: request.models.clone(),
        };

        config
            .providers
            .insert(request.name.clone(), provider_config.clone());

        Ok(ProviderInfo {
            name: request.name,
            api_key: provider_config.api_key,
            base_url: provider_config.base_url,
            models: provider_config.models,
        })
    }

    pub async fn update_model(
        &self,
        provider_name: &str,
        model_name: &str,
        update: UpdateModelRequest,
    ) -> Result<ModelInfo, ServerError> {
        let mut config = self.state.config.write().await;

        let provider = config
            .providers
            .get_mut(provider_name)
            .ok_or_else(|| ServerError::not_found("Provider", provider_name))?;

        let model = provider
            .models
            .iter_mut()
            .find(|m| m.name == model_name)
            .ok_or_else(|| {
            ServerError::not_found(
                format!("Model in provider '{}'", provider_name),
                model_name,
            )
        })?;

        if let Some(description) = update.description {
            model.description = Some(description);
        }
        if let Some(context_window) = update.context_window {
            model.context_window = Some(context_window);
        }
        if let Some(temperature) = update.temperature {
            model.temperature = Some(temperature);
        }
        if let Some(max_tokens) = update.max_tokens {
            model.max_tokens = Some(max_tokens);
        }
        if let Some(api_key) = update.api_key {
            provider.api_key = Some(api_key);
        }
        if let Some(base_url) = update.base_url {
            provider.base_url = Some(base_url);
        }

        Ok(ModelInfo {
            provider: provider_name.to_string(),
            name: model.name.clone(),
            description: model.description.clone(),
            context_window: model.context_window,
            temperature: model.temperature,
            max_tokens: model.max_tokens,
            api_key: provider.api_key.clone(),
            base_url: provider.base_url.clone(),
        })
    }

    pub async fn delete_provider(&self, name: &str) -> Result<(), ServerError> {
        let mut config = self.state.config.write().await;

        if config.providers.remove(name).is_none() {
            return Err(ServerError::not_found("Provider", name));
        }

        Ok(())
    }
}
