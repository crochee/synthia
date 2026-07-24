//! The [`OpenAICompatibleProvider`] struct + the 2
//! constructors (`new` / `with_api_key`).
//!
//! All 8 transform / response / request methods are split
//! across [`super::transform`], [`super::tool_message`],
//! [`super::content`], [`super::response`], and
//! [`super::request`].

use crate::types::ModelConfig;

pub struct OpenAICompatibleProvider {
    pub(in crate::openai) api_key: Option<String>,
    pub(in crate::openai) base_url: String,
    pub(in crate::openai) model_config: ModelConfig,
    pub(in crate::openai) client: reqwest::Client,
}

impl OpenAICompatibleProvider {
    pub fn new(base_url: String, model_config: ModelConfig) -> Self {
        Self {
            api_key: None,
            base_url,
            model_config,
            client: reqwest::Client::new(),
        }
    }

    pub fn with_api_key(mut self, key: &str) -> Self {
        self.api_key = Some(key.to_string());
        self
    }
}
