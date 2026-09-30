//! Provider and model configuration types

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    #[serde(default)]
    pub models: Vec<ModelConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelConfig {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub context_window: Option<usize>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- ProviderConfig 3-field struct -----------------------------

    /// `ProviderConfig::models` defaults to an empty Vec when
    /// the field is omitted (the `#[serde(default)]` contract).
    /// Note: `api_key` and `base_url` are NOT defaulted — they
    /// are `Option<String>` so they default to `None` via the
    /// type's null-handling.
    #[test]
    fn provider_config_defaults_models_to_empty_vec() {
        let p: ProviderConfig = serde_json::from_str("{}").unwrap();
        assert!(p.api_key.is_none());
        assert!(p.base_url.is_none());
        assert!(p.models.is_empty());
    }

    /// `ProviderConfig` MUST round-trip all 3 fields through JSON.
    #[test]
    fn provider_config_round_trips_through_json() {
        let p = ProviderConfig {
            api_key: Some("sk-test".to_string()),
            base_url: Some("https://api.example.com".to_string()),
            models: vec![ModelConfig {
                name: "gpt-4o".to_string(),
                description: Some("flagship".to_string()),
                context_window: Some(128_000),
                temperature: Some(0.5),
                max_tokens: Some(4_096),
            }],
        };
        let json = serde_json::to_string(&p).unwrap();
        let parsed: ProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.api_key, p.api_key);
        assert_eq!(parsed.base_url, p.base_url);
        assert_eq!(parsed.models.len(), 1);
    }

    /// `ProviderConfig::models` MUST support multiple entries
    /// (the typical multi-model setup).
    #[test]
    fn provider_config_supports_multiple_models() {
        let p = ProviderConfig {
            api_key: None,
            base_url: None,
            models: vec![
                ModelConfig {
                    name: "small".to_string(),
                    description: None,
                    context_window: Some(8_000),
                    temperature: None,
                    max_tokens: None,
                },
                ModelConfig {
                    name: "large".to_string(),
                    description: None,
                    context_window: Some(200_000),
                    temperature: None,
                    max_tokens: None,
                },
            ],
        };
        let json = serde_json::to_string(&p).unwrap();
        let parsed: ProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.models.len(), 2);
        assert_eq!(parsed.models[0].name, "small");
        assert_eq!(parsed.models[1].name, "large");
    }

    /// `ProviderConfig` MUST serialize with exactly 3 keys
    /// (no extras, no missing).
    #[test]
    fn provider_config_serializes_with_exact_three_keys() {
        let p = ProviderConfig {
            api_key: None,
            base_url: None,
            models: vec![],
        };
        let json: serde_json::Value = serde_json::to_value(&p).unwrap();
        let obj = json.as_object().expect("must be object");
        assert_eq!(obj.len(), 3);
        assert!(obj.contains_key("api_key"));
        assert!(obj.contains_key("base_url"));
        assert!(obj.contains_key("models"));
    }

    // -- ModelConfig 5-field struct --------------------------------

    /// `ModelConfig` MUST default all 4 optional fields to
    /// `None` when only `name` is supplied.
    #[test]
    fn model_config_minimal_serde_defaults_optionals() {
        let json = r#"{"name": "claude-opus"}"#;
        let m: ModelConfig = serde_json::from_str(json).unwrap();
        assert_eq!(m.name, "claude-opus");
        assert!(m.description.is_none());
        assert!(m.context_window.is_none());
        assert!(m.temperature.is_none());
        assert!(m.max_tokens.is_none());
    }

    /// `ModelConfig` MUST round-trip all 5 fields through JSON.
    #[test]
    fn model_config_round_trips_through_json() {
        let m = ModelConfig {
            name: "claude-sonnet".to_string(),
            description: Some("balanced".to_string()),
            context_window: Some(200_000),
            temperature: Some(0.7),
            max_tokens: Some(8_192),
        };
        let json = serde_json::to_string(&m).unwrap();
        let parsed: ModelConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "claude-sonnet");
        assert_eq!(parsed.description, Some("balanced".to_string()));
        assert_eq!(parsed.context_window, Some(200_000));
        assert_eq!(parsed.temperature, Some(0.7));
        assert_eq!(parsed.max_tokens, Some(8_192));
    }

    /// `ModelConfig::temperature` MUST accept negative values
    /// (`f32` covers the full range; some providers use negative
    /// temperatures for "less random" sampling).
    #[test]
    fn model_config_temperature_accepts_negative_values() {
        let json = r#"{"name": "m", "temperature": -0.5}"#;
        let m: ModelConfig = serde_json::from_str(json).unwrap();
        assert_eq!(m.temperature, Some(-0.5));
    }

    /// `ModelConfig::context_window` accepts positive values up to
    /// `i64::MAX` (no overflow panic). Pin the contract: the
    /// deserializer preserves the literal value as a positive
    /// `usize`. NOTE: serde_json does NOT saturate; inputting
    /// `9223372036854775807` (i64::MAX) yields exactly that
    /// value (since usize is u64 on 64-bit targets).
    #[test]
    fn model_config_context_window_accepts_i64_max() {
        let json = r#"{"name": "m", "context_window": 9223372036854775807}"#;
        let m: ModelConfig = serde_json::from_str(json).unwrap();
        assert_eq!(m.context_window, Some(9_223_372_036_854_775_807));
    }

    /// `ModelConfig` MUST support `Debug + Clone`.
    #[test]
    fn model_config_supports_debug_and_clone() {
        let m = ModelConfig {
            name: "x".to_string(),
            description: Some("d".to_string()),
            context_window: Some(1),
            temperature: Some(0.0),
            max_tokens: Some(2),
        };
        let _ = format!("{m:?}");
        let cloned = m.clone();
        assert_eq!(cloned.name, m.name);
    }
}
