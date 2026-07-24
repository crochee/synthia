use synthia_core::{Error, Registry};

use super::ModelRouter;

impl ModelRouter {
    pub fn is_model_available(&self, model: &str) -> bool {
        self.providers.contains_key(model)
    }

    pub fn fallback_to_backup(
        &self,
        primary_model: &str,
    ) -> Result<String, Error> {
        if let Some(backup_models) = self.fallback_chains.get(primary_model) {
            for backup_model in backup_models {
                if self.is_model_available(backup_model) {
                    tracing::warn!(
                        primary_model = %primary_model,
                        backup_model = %backup_model,
                        "Primary model unavailable, falling back to backup"
                    );
                    return Ok(backup_model.clone());
                }
            }
        }
        Err(Error::Router("No available model".into()))
    }

    pub async fn check_model_availability(&self, model: &str) -> bool {
        if !self.providers.contains_key(model) {
            return false;
        }

        if let Some(ref registry) = self.provider_registry
            && let Ok(_provider_info) = registry.get(model).await
        {
            return true;
        }

        true
    }

    pub async fn execute_with_fallback(
        &self,
        primary_model: &str,
        _request: &crate::CompletionRequest,
    ) -> Result<(String, crate::CompletionResponse), Error> {
        let registry = self.provider_registry.as_ref().ok_or_else(|| {
            Error::Router("No provider registry attached".to_string())
        })?;

        let mut candidates = vec![primary_model.to_string()];
        if let Some(fallbacks) = self.fallback_chains.get(primary_model) {
            candidates.extend(fallbacks.iter().cloned());
        }

        let mut last_error: Option<String> = None;

        for model in &candidates {
            if registry.get(model).await.is_ok() {
                tracing::info!(model = %model, "Model available in registry");
                last_error = Some(format!(
                    "Model {} not directly executable via Registry",
                    model
                ));
            }
        }

        tracing::error!(
            models = ?candidates,
            last_error = ?last_error,
            "Fallback chain requires provider lookup capability"
        );
        Err(Error::Router("All models in fallback chain failed".into()))
    }
}
