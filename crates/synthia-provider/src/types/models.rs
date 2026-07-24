//! Token accounting + provider/model metadata structs.
//!
//! [`TokenUsage`] is the canonical usage type used by every
//! crate. Three downstream crates (`synthia-session`,
//! `synthia-agent`, `synthia-context`) re-export this type via
//! 1-line `pub use` shims.

use serde::{Deserialize, Serialize};
use synthia_telemetry::Sensitive;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
    #[serde(default)]
    pub cached_prompt_tokens: Option<usize>,
    /// KV cache read tokens (Anthropic `cache_read_input_tokens`).
    /// Used for cache hit ratio computation. `None` when the
    /// provider does not report cache metrics.
    #[serde(default)]
    pub cache_read_tokens: Option<usize>,
    /// KV cache write tokens (Anthropic `cache_creation_input_tokens`).
    /// `None` when the provider does not report cache metrics.
    #[serde(default)]
    pub cache_write_tokens: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct ModelInfo {
    pub name: String,
    pub provider: String,
    pub context_window: usize,
    pub max_output_tokens: usize,
    pub supports_tools: bool,
    pub supports_streaming: bool,
    pub supports_vision: bool,
}

#[derive(Clone, Debug)]
pub struct ProviderInfo {
    pub name: String,
    pub models: Vec<ModelInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub api_key: Sensitive<String>,
    pub base_url: Option<String>,
    pub timeout_ms: Option<u64>,
    pub max_retries: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct ModelConfig {
    pub name: String,
    pub provider: String,
    pub context_window: usize,
    pub max_output_tokens: usize,
    pub supports_tools: bool,
    pub supports_streaming: bool,
    pub supports_reasoning: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_usage_serializes_new_cache_fields() {
        let usage = TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            cached_prompt_tokens: Some(80),
            cache_read_tokens: Some(80),
            cache_write_tokens: Some(20),
        };
        let json = serde_json::to_value(&usage).unwrap();
        assert!(json.get("cache_read_tokens").is_some());
        assert!(json.get("cache_write_tokens").is_some());
    }

    #[test]
    fn test_token_usage_defaults_new_fields_to_none() {
        let old_json =
            r#"{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}"#;
        let usage: TokenUsage = serde_json::from_str(old_json).unwrap();
        assert_eq!(usage.cache_read_tokens, None);
        assert_eq!(usage.cache_write_tokens, None);
    }
}
