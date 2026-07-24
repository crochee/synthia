//! The [`AnthropicProvider`] struct + the 3 constructors
//! (`new` / `with_api_key` / `with_base_url`).
//!
//! All transform / response / request methods are split
//! across [`super::transform`], [`super::parse`], and
//! [`super::request`].

use crate::types::ModelConfig;

#[derive(Debug)]
pub struct AnthropicProvider {
    pub(in crate::anthropic) api_key: Option<String>,
    pub(in crate::anthropic) model_config: ModelConfig,
    pub(in crate::anthropic) client: reqwest::Client,
    pub(in crate::anthropic) base_url: String,
    /// Stateful cache-policy applier that short-circuits
    /// `apply_cache_policy` when the request's `tools` / `messages`
    /// `Arc` references are identical to the previous call. Uses
    /// interior mutability because [`transform_request`] takes `&self`.
    pub(in crate::anthropic) cache_policy_applier:
        parking_lot::Mutex<crate::cache_policy::CachePolicyApplier>,
}

impl AnthropicProvider {
    pub fn new(model_config: ModelConfig) -> Self {
        Self {
            api_key: None,
            model_config,
            client: reqwest::Client::new(),
            base_url: "https://api.anthropic.com".to_string(),
            cache_policy_applier: parking_lot::Mutex::new(
                crate::cache_policy::CachePolicyApplier::new(),
            ),
        }
    }

    pub fn with_api_key(mut self, key: &str) -> Self {
        self.api_key = Some(key.to_string());
        self
    }

    pub fn with_base_url(mut self, base_url: &str) -> Self {
        self.base_url = base_url.trim_end_matches('/').to_string();
        self
    }
}
