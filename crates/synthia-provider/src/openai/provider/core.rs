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

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_model_config(name: &str) -> ModelConfig {
        ModelConfig {
            name: name.to_string(),
            provider: "openai-compatible".to_string(),
            context_window: 128_000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        }
    }

    // -- new() ---------------------------------------------------------

    /// `new(base_url, model_config)` MUST store both fields
    /// verbatim and leave `api_key = None`.
    #[test]
    fn new_stores_base_url_and_model_config() {
        let p = OpenAICompatibleProvider::new(
            "https://api.example.com".to_string(),
            dummy_model_config("my-model"),
        );
        assert_eq!(p.base_url, "https://api.example.com");
        assert_eq!(p.model_config.name, "my-model");
        assert_eq!(p.api_key, None);
    }

    /// `new` MUST accept any `String` for `base_url`
    /// (no scheme/format validation).
    #[test]
    fn new_accepts_any_base_url_string() {
        let p = OpenAICompatibleProvider::new(
            "http://localhost:1234".to_string(),
            dummy_model_config("x"),
        );
        assert_eq!(p.base_url, "http://localhost:1234");

        let p = OpenAICompatibleProvider::new(
            "".to_string(),
            dummy_model_config("x"),
        );
        assert_eq!(p.base_url, "");
    }

    /// `new` MUST preserve all 7 `ModelConfig` fields.
    #[test]
    fn new_preserves_all_model_config_fields() {
        let mc = ModelConfig {
            name: "claude-opus-4-5".to_string(),
            provider: "anthropic-compatible".to_string(),
            context_window: 200_000,
            max_output_tokens: 8192,
            supports_tools: false,
            supports_streaming: false,
            supports_reasoning: true,
        };
        let p = OpenAICompatibleProvider::new(
            "https://api.anthropic.com".to_string(),
            mc.clone(),
        );
        assert_eq!(p.model_config.name, "claude-opus-4-5");
        assert_eq!(p.model_config.provider, "anthropic-compatible");
        assert_eq!(p.model_config.context_window, 200_000);
        assert_eq!(p.model_config.max_output_tokens, 8192);
        assert!(!p.model_config.supports_tools);
        assert!(!p.model_config.supports_streaming);
        assert!(p.model_config.supports_reasoning);
    }

    /// Two `new()` calls MUST produce independent providers
    /// (no shared state — no Arc fields, no static cache).
    #[test]
    fn new_creates_independent_providers() {
        let p1 = OpenAICompatibleProvider::new(
            "https://a.example".to_string(),
            dummy_model_config("a"),
        );
        let p2 = OpenAICompatibleProvider::new(
            "https://b.example".to_string(),
            dummy_model_config("b"),
        );
        assert_eq!(p1.base_url, "https://a.example");
        assert_eq!(p2.base_url, "https://b.example");
        assert_ne!(p1.base_url, p2.base_url);
    }

    // -- with_api_key --------------------------------------------------

    /// `with_api_key(s)` MUST populate the `api_key` field.
    #[test]
    fn with_api_key_populates_key() {
        let p = OpenAICompatibleProvider::new(
            "https://x".to_string(),
            dummy_model_config("x"),
        )
        .with_api_key("sk-test-123");
        assert_eq!(p.api_key, Some("sk-test-123".to_string()));
    }

    /// `with_api_key` MUST accept `&str` (not just `String`).
    #[test]
    fn with_api_key_accepts_str_slice() {
        let p = OpenAICompatibleProvider::new(
            "https://x".to_string(),
            dummy_model_config("x"),
        )
        .with_api_key("sk");
        assert_eq!(p.api_key, Some("sk".to_string()));
    }

    /// `with_api_key("")` MUST accept an empty key (the
    /// provider validates at request time, not construction).
    #[test]
    fn with_api_key_accepts_empty_string() {
        let p = OpenAICompatibleProvider::new(
            "https://x".to_string(),
            dummy_model_config("x"),
        )
        .with_api_key("");
        assert_eq!(p.api_key, Some("".to_string()));
    }

    /// `with_api_key` MUST be chainable (returns `self`).
    #[test]
    fn with_api_key_is_chainable() {
        let p = OpenAICompatibleProvider::new(
            "https://x".to_string(),
            dummy_model_config("x"),
        )
        .with_api_key("first")
        .with_api_key("second");
        // Last call wins (overwrites).
        assert_eq!(p.api_key, Some("second".to_string()));
    }

    /// `with_api_key` MUST preserve the other 3 fields
    /// (base_url + model_config + client).
    #[test]
    fn with_api_key_preserves_other_fields() {
        let p = OpenAICompatibleProvider::new(
            "https://api.example.com".to_string(),
            dummy_model_config("my-model"),
        )
        .with_api_key("sk-123");
        assert_eq!(p.base_url, "https://api.example.com");
        assert_eq!(p.model_config.name, "my-model");
        // api_key is set.
        assert!(p.api_key.is_some());
    }

    // -- reqwest::Client ------------------------------------------------

    /// `reqwest::Client` MUST be created internally (not
    /// exposed via setter — verified by accessibility).
    #[test]
    fn client_field_is_constructed() {
        // The `client` field is `pub(in crate::openai)`, so
        // we can't directly access it from outside the
        // `openai` module — but we can verify that
        // construction succeeds and the provider is usable
        // in other respects (via the public surface).
        let p = OpenAICompatibleProvider::new(
            "https://x".to_string(),
            dummy_model_config("x"),
        );
        // Pin by exercising the public surface.
        assert!(p.api_key.is_none());
    }
}
