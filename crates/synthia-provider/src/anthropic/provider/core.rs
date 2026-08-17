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

#[cfg(test)]
mod tests {
    use super::*;

    fn model_config(name: &str) -> ModelConfig {
        ModelConfig {
            name: name.to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4_096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        }
    }

    // -- Constructor defaults ------------------------------------------

    /// `AnthropicProvider::new(model_config)` MUST default
    /// `api_key = None` and `base_url = "https://api.anthropic.com"`.
    #[test]
    fn new_defaults_api_key_and_base_url() {
        let p = AnthropicProvider::new(model_config("claude-opus"));
        assert!(p.api_key.is_none());
        assert_eq!(p.base_url, "https://api.anthropic.com");
        assert_eq!(p.model_config.name, "claude-opus");
        let _ = &p.client; // pin pub(in crate::anthropic) access.
    }

    /// `new` MUST store the supplied `ModelConfig` verbatim
    /// (no field mutation, no name rewriting).
    #[test]
    fn new_stores_model_config_verbatim() {
        let cfg = ModelConfig {
            name: "claude-sonnet-4-5".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200_000,
            max_output_tokens: 4_096,
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: false,
        };
        let p = AnthropicProvider::new(cfg);
        assert_eq!(p.model_config.name, "claude-sonnet-4-5");
        assert_eq!(p.model_config.provider, "anthropic");
        assert_eq!(p.model_config.context_window, 200_000);
        assert_eq!(p.model_config.max_output_tokens, 4_096);
        assert!(p.model_config.supports_tools);
        assert!(p.model_config.supports_streaming);
        assert!(!p.model_config.supports_reasoning);
    }

    /// `new` MUST create a fresh cache policy applier for each
    /// provider instance (no cross-instance state).
    #[test]
    fn new_creates_independent_cache_policy_applier_per_instance() {
        let a = AnthropicProvider::new(model_config("a"));
        let b = AnthropicProvider::new(model_config("b"));
        // Different instances MUST have different Mutex addresses.
        let a_addr = std::ptr::addr_of!(a.cache_policy_applier) as usize;
        let b_addr = std::ptr::addr_of!(b.cache_policy_applier) as usize;
        assert_ne!(a_addr, b_addr);
    }

    // -- with_api_key --------------------------------------------------

    /// `with_api_key` MUST populate `api_key` with the supplied
    /// string and consume `self` (builder pattern).
    #[test]
    fn with_api_key_populates_key() {
        let p = AnthropicProvider::new(model_config("c"))
            .with_api_key("sk-ant-test-123");
        assert_eq!(p.api_key, Some("sk-ant-test-123".to_string()));
    }

    /// `with_api_key` MUST overwrite a previously-set key
    /// (builder pattern is mutable).
    #[test]
    fn with_api_key_overwrites_previous_key() {
        let p = AnthropicProvider::new(model_config("c"))
            .with_api_key("first")
            .with_api_key("second");
        assert_eq!(p.api_key, Some("second".to_string()));
    }

    /// `with_api_key` MUST accept an empty string (no
    /// validation — callers must check themselves).
    #[test]
    fn with_api_key_accepts_empty_string() {
        let p = AnthropicProvider::new(model_config("c")).with_api_key("");
        assert_eq!(p.api_key, Some(String::new()));
    }

    // -- with_base_url -------------------------------------------------

    /// `with_base_url` MUST store the URL with any trailing
    /// slash stripped (so subsequent `endpoint + path` joins
    /// don't double up).
    #[test]
    fn with_base_url_strips_single_trailing_slash() {
        let p = AnthropicProvider::new(model_config("c"))
            .with_base_url("https://api.example.com/");
        assert_eq!(p.base_url, "https://api.example.com");
    }

    /// `with_base_url` MUST strip ALL trailing slashes (not
    /// just one — defensive).
    #[test]
    fn with_base_url_strips_all_trailing_slashes() {
        let p = AnthropicProvider::new(model_config("c"))
            .with_base_url("https://api.example.com////");
        assert_eq!(p.base_url, "https://api.example.com");
    }

    /// `with_base_url` MUST leave URLs without trailing
    /// slashes unchanged.
    #[test]
    fn with_base_url_preserves_url_without_trailing_slash() {
        let p = AnthropicProvider::new(model_config("c"))
            .with_base_url("https://api.example.com");
        assert_eq!(p.base_url, "https://api.example.com");
    }

    /// `with_base_url` MUST support both http:// and https://
    /// schemes (no scheme restriction).
    #[test]
    fn with_base_url_accepts_http_and_https_schemes() {
        let p1 = AnthropicProvider::new(model_config("c"))
            .with_base_url("http://localhost:8080");
        assert_eq!(p1.base_url, "http://localhost:8080");
        let p2 = AnthropicProvider::new(model_config("c"))
            .with_base_url("https://api.example.com/v1/");
        assert_eq!(p2.base_url, "https://api.example.com/v1");
    }

    /// `with_base_url` MUST support overwriting a previous URL
    /// (builder pattern is mutable).
    #[test]
    fn with_base_url_overwrites_previous_url() {
        let p = AnthropicProvider::new(model_config("c"))
            .with_base_url("https://first.example.com/")
            .with_base_url("https://second.example.com");
        assert_eq!(p.base_url, "https://second.example.com");
    }

    /// `with_base_url` MUST accept an empty string and store
    /// it as empty (no implicit default).
    #[test]
    fn with_base_url_accepts_empty_string() {
        let p = AnthropicProvider::new(model_config("c")).with_base_url("");
        assert_eq!(p.base_url, "");
    }

    // -- Combined builder pattern --------------------------------------

    /// The full builder chain MUST produce a fully-populated
    /// provider that overrides every default field.
    #[test]
    fn full_builder_chain_overrides_all_defaults() {
        let p = AnthropicProvider::new(model_config("claude-opus"))
            .with_api_key("k")
            .with_base_url("https://proxy.example.com/");
        assert_eq!(p.api_key, Some("k".to_string()));
        assert_eq!(p.base_url, "https://proxy.example.com");
        assert_eq!(p.model_config.name, "claude-opus");
    }

    // -- Trait surface --------------------------------------------------

    /// `AnthropicProvider` MUST derive `Debug`. Pin the format
    /// (which fields appear) since debug output may show up in
    /// trace dumps.
    #[test]
    fn anthropic_provider_implements_debug() {
        let p = AnthropicProvider::new(model_config("c")).with_api_key("k");
        let dbg = format!("{p:?}");
        assert!(dbg.contains("AnthropicProvider"));
        assert!(dbg.contains("api_key"));
        assert!(dbg.contains("model_config"));
    }
}
