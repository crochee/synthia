//! Synthia Provider: LLM provider abstraction with OpenAI-compatible and Anthropic implementations
//!
//! The two shipped adapters are feature-gated (`anthropic` / `openai`,
//! both on by default) because each carries an HTTP client and its
//! transport stack. Everything else — the [`traits::ModelProvider`]
//! trait, the wire types, the token estimator, the typed profiles, the
//! retry classifier, `json_repair` — compiles without either, so a
//! consumer with their own provider compiles no HTTP at all:
//!
//! ```toml
//! synthia-provider = { version = "0.1", default-features = false }
//! ```
#![allow(clippy::result_large_err)]

#[cfg(feature = "anthropic")]
pub mod anthropic;
pub mod assembler;
pub mod cache_mark;
pub mod cache_policy;
pub mod config;
pub mod context_overflow;
pub mod credential;
pub mod error_body;
pub mod json_repair;
#[cfg(feature = "openai")]
pub mod openai;
#[cfg(feature = "openai")]
pub(crate) mod openai_streaming;
pub mod profile;
pub mod retry;
pub mod streaming;
pub mod tier;
pub mod token_counter;
pub mod traits;
pub mod traits_stub;
pub mod types;
pub mod validation;

pub use profile::{
    AnthropicProfile,
    DEFAULT_ANTHROPIC_CONTEXT_WINDOW,
    DEFAULT_ANTHROPIC_MAX_OUTPUT_TOKENS,
    DEFAULT_OPENAI_CONTEXT_WINDOW,
    DEFAULT_OPENAI_MAX_OUTPUT_TOKENS,
    ModelCapabilities,
    OpenAIProfile,
    ProviderProfile,
    ProviderRegistry,
    StubProfile,
};

#[cfg(test)]
mod tests;
#[cfg(feature = "anthropic")]
pub use anthropic::AnthropicProvider;
pub use assembler::BlockAssembler;
pub use cache_mark::{CacheControlMark, CacheScope, CacheTtl};
pub use cache_policy::{
    CachePolicy,
    CachePolicyApplier,
    MessageCacheStrategy,
    apply_cache_policy,
};
pub use config::{ProviderEntry, WorkspaceConfig};
pub use context_overflow::{
    ContextOverflowDetector,
    is_silent_overflow,
    synthesize_orphan_result,
};
pub use credential::{
    CredentialError,
    CredentialStatus,
    classify,
    normalize_api_key,
};
pub use error_body::{
    MAX_PROVIDER_ERROR_BODY_CHARS,
    ProviderErrorBody,
    parse_provider_error_body,
};
pub use json_repair::{
    ToolArgsQuality,
    completion::{complete_partial_json, parse_tool_input_with_completion},
    parse_tool_input,
    parse_tool_input_logged,
    parse_tool_input_reported,
    repair_json,
};
#[cfg(feature = "openai")]
pub use openai::OpenAICompatibleProvider;
pub use retry::{
    RetryClass,
    RetryConfig,
    classify_error,
    classify_provider_error_body,
    is_retryable_error,
    parse_retry_after,
    retry_config_for,
    retry_with_backoff,
    retry_with_classification,
    retry_with_retry_after,
};
#[cfg(feature = "anthropic")]
pub use streaming::{
    AnthropicStreamContentBlock,
    AnthropicStreamDelta,
    AnthropicStreamEvent,
};
pub use streaming::{
    DEFAULT_STREAM_IDLE_TIMEOUT,
    IDLE_TIMEOUT_MARKER,
    idle_timeout_error,
    is_idle_timeout,
};
pub use tier::{ModelTier, TierLimits, TierParseError};
pub use token_counter::{TokenCounter, estimate_messages_token_count};
pub use traits::{ModelProvider, StreamResult, completion_to_sampling};
pub use types::{
    BilledClass,
    CompletionRequest,
    CompletionResponse,
    Content,
    ContentPart,
    ImageContent,
    ImageDetail,
    Message,
    MessageKind,
    ModelConfig,
    ModelInfo,
    ProviderConfig,
    ProviderInfo,
    ReasoningContent,
    ResourceLink,
    Role,
    SamplingResult,
    StreamChunk,
    TextContent,
    TokenUsage,
    ToolAnnotations,
    ToolChoice,
    ToolDefinition,
    ToolResult,
    ToolUse,
};
pub use validation::{
    EMPTY_RESPONSE_MARKER,
    empty_response_error,
    is_empty_response_error,
};
