//! Token accounting + provider/model metadata structs.
//!
//! [`TokenUsage`] is the canonical usage type used by every
//! crate. Three downstream crates (`synthia-session`,
//! `synthia-harness`, `synthia-context`) re-export this type via
//! 1-line `pub use` shims.

use serde::{Deserialize, Serialize};
use synthia_core::Sensitive;

/// Per-call token accounting. Mirrors the dsh `Usage` 5-bucket split:
///
/// | field               | dsh equivalent     | provider name                               |
/// |---------------------|--------------------|---------------------------------------------|
/// | `prompt_tokens`     | `input_tokens`     | `prompt_tokens` (OpenAI/Anthropic)          |
/// | `completion_tokens` | `output_tokens`    | `completion_tokens` (OpenAI/Anthropic)     |
/// | `cache_read_tokens` | `cache_read`       | `cache_read_input_tokens` (Anthropic) / `cached_tokens` (OpenAI) |
/// | `cache_write_tokens`| `cache_creation`   | `cache_creation_input_tokens` (Anthropic)  |
/// | `reasoning_tokens`  | `reasoning`        | `completion_tokens_details.reasoning_tokens` (OpenAI) |
///
/// Each non-required field is `None` when the provider does not
/// surface it. Downstream cost attribution reads
/// [`TokenUsage::for_each_bucket`] to multiply bucket counts by a
/// per-class price table without hand-rolling a match over the
/// five `Option`s.
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
    /// Reasoning / thinking tokens (OpenAI
    /// `completion_tokens_details.reasoning_tokens`). Billed as
    /// output but reported separately so the usage meter can
    /// attribute cognition cost. `None` when the provider does
    /// not report it.
    #[serde(default)]
    pub reasoning_tokens: Option<usize>,
}

/// One of the five `TokenUsage` cost buckets.
///
/// Used by lib consumers to apply per-class pricing without poking
/// at the `Option<usize>` fields one by one. Variants are ordered to
/// match the dsh `Usage` key order so the JSON contract lines up.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum BilledClass {
    /// `prompt_tokens` (always known)
    Input,
    /// `cache_read_tokens` (provider optional)
    CacheRead,
    /// `cache_write_tokens` (provider optional)
    CacheWrite,
    /// `reasoning_tokens` (provider optional)
    Reasoning,
    /// `completion_tokens` minus `reasoning_tokens` when both known,
    /// otherwise `completion_tokens` in full.
    Output,
}

impl BilledClass {
    /// Stable lowercase token used in JSON contracts and the
    /// OpenAPI schema; matches dsh `Usage` key casing.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::CacheRead => "cache_read",
            Self::CacheWrite => "cache_write",
            Self::Reasoning => "reasoning",
            Self::Output => "output",
        }
    }
}

impl TokenUsage {
    /// Read the bucket count for one [`BilledClass`]. `None`
    /// buckets return `None`; `Output` always returns `Some` because
    /// it is derived from `completion_tokens - reasoning_tokens`.
    pub fn bucket(&self, class: BilledClass) -> Option<usize> {
        match class {
            BilledClass::Input => Some(self.prompt_tokens),
            BilledClass::CacheRead => self.cache_read_tokens,
            BilledClass::CacheWrite => self.cache_write_tokens,
            BilledClass::Reasoning => self.reasoning_tokens,
            BilledClass::Output => Some(
                self.completion_tokens
                    .saturating_sub(self.reasoning_tokens.unwrap_or(0)),
            ),
        }
    }

    /// Fold each billed bucket through `f`. Buckets with `None`
    /// values are skipped. Lets a downstream cost module write
    /// `usage.for_each_bucket(|class, n| price[class] * n)` without
    /// hand-rolling a five-arm match.
    pub fn for_each_bucket<F>(&self, mut f: F)
    where
        F: FnMut(BilledClass, usize),
    {
        for class in [
            BilledClass::Input,
            BilledClass::CacheRead,
            BilledClass::CacheWrite,
            BilledClass::Reasoning,
            BilledClass::Output,
        ] {
            if let Some(n) = self.bucket(class) {
                f(class, n);
            }
        }
    }

    /// Sum every bucket count (treating `None` as `0`). Sums to
    /// `total_tokens + cache_read + cache_write` in the common case.
    #[allow(clippy::missing_panics_doc)]
    pub fn bucket_total(&self) -> usize {
        let mut total = 0_usize;
        self.for_each_bucket(|_, n| total = total.saturating_add(n));
        total
    }
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
            reasoning_tokens: None,
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

    #[test]
    fn test_billed_class_as_str_is_stable() {
        assert_eq!(BilledClass::Input.as_str(), "input");
        assert_eq!(BilledClass::CacheRead.as_str(), "cache_read");
        assert_eq!(BilledClass::CacheWrite.as_str(), "cache_write");
        assert_eq!(BilledClass::Reasoning.as_str(), "reasoning");
        assert_eq!(BilledClass::Output.as_str(), "output");
    }

    #[test]
    fn test_token_usage_bucket_output_subtracts_reasoning() {
        let usage = TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            cached_prompt_tokens: Some(80),
            cache_read_tokens: Some(80),
            cache_write_tokens: Some(20),
            reasoning_tokens: Some(30),
        };
        assert_eq!(usage.bucket(BilledClass::Output), Some(20));
        assert_eq!(usage.bucket(BilledClass::Reasoning), Some(30));
        assert_eq!(usage.bucket(BilledClass::Input), Some(100));
    }

    #[test]
    fn test_token_usage_bucket_skips_missing_optionals() {
        let usage = TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            reasoning_tokens: None,
        };
        assert_eq!(usage.bucket(BilledClass::CacheRead), None);
        assert_eq!(usage.bucket(BilledClass::CacheWrite), None);
        assert_eq!(usage.bucket(BilledClass::Reasoning), None);
        assert_eq!(usage.bucket(BilledClass::Output), Some(5));
    }

    #[test]
    fn test_token_usage_for_each_bucket_visits_only_some() {
        let usage = TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            cached_prompt_tokens: None,
            cache_read_tokens: Some(7),
            cache_write_tokens: None,
            reasoning_tokens: None,
        };
        let mut seen = Vec::new();
        usage.for_each_bucket(|class, n| seen.push((class, n)));
        assert_eq!(
            seen,
            vec![
                (BilledClass::Input, 10),
                (BilledClass::CacheRead, 7),
                (BilledClass::Output, 5),
            ]
        );
    }

    #[test]
    fn test_token_usage_bucket_total_matches_some_sum() {
        let usage = TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            cached_prompt_tokens: None,
            cache_read_tokens: Some(7),
            cache_write_tokens: Some(3),
            reasoning_tokens: Some(2),
        };
        // 10 + 7 + 3 + 2 + (5 - 2) = 25
        assert_eq!(usage.bucket_total(), 25);
    }
}
