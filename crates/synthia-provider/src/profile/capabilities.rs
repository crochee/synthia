//! Shared capability flags + the per-backend default token budgets
//! profiles fall back to when they carry no explicit override.

use serde::{Deserialize, Serialize};

/// Default context window for OpenAI-compatible profiles that don't
/// override it. Mirrors the GPT-4o / GPT-4-turbo class default.
pub const DEFAULT_OPENAI_CONTEXT_WINDOW: usize = 128_000;

/// Default context window for Anthropic profiles that don't override
/// it. Mirrors the Claude Sonnet / Opus class default.
pub const DEFAULT_ANTHROPIC_CONTEXT_WINDOW: usize = 200_000;

/// Default `max_output_tokens` for OpenAI-compatible profiles.
pub const DEFAULT_OPENAI_MAX_OUTPUT_TOKENS: usize = 4_096;

/// Default `max_output_tokens` for Anthropic profiles.
pub const DEFAULT_ANTHROPIC_MAX_OUTPUT_TOKENS: usize = 8_192;

/// Common capability flags shared by every concrete profile.
///
/// Profiles that need to deviate (e.g. stub providers, image-only
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// Whether the model accepts tool definitions and emits tool calls.
    pub supports_tools: bool,
    /// Whether the provider streams incremental chunks
    /// (`complete_with_stream` returns a usable stream).
    pub supports_streaming: bool,
    /// Whether the model emits explicit reasoning / thinking tokens.
    pub supports_reasoning: bool,
    /// Default `max_output_tokens` the model is configured for.
    pub max_output_tokens: usize,
    /// Default `context_window` the model is configured for.
    pub context_window: usize,
}

impl ModelCapabilities {
    /// Capability profile for the current generation of frontier
    /// chat models (tool calling + streaming + reasoning).
    pub const fn frontier_chat() -> Self {
        Self {
            supports_tools: true,
            supports_streaming: true,
            supports_reasoning: true,
            max_output_tokens: DEFAULT_OPENAI_MAX_OUTPUT_TOKENS,
            context_window: DEFAULT_OPENAI_CONTEXT_WINDOW,
        }
    }
}
