//! Synthia Provider: LLM provider abstraction with OpenAI-compatible and Anthropic implementations

pub mod anthropic;
pub mod cache_mark;
pub mod cache_policy;
pub mod config;
pub mod context_overflow;
pub mod openai;
pub(crate) mod openai_streaming;
pub mod retry;
pub mod streaming;
pub mod token_counter;
pub mod traits;
pub mod traits_stub;
pub mod types;
pub mod validation;

#[cfg(test)]
mod tests;

pub use anthropic::AnthropicProvider;
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
pub use openai::OpenAICompatibleProvider;
pub use retry::{
    RetryConfig,
    is_retryable_error,
    parse_retry_after,
    retry_with_backoff,
    retry_with_retry_after,
};
pub use streaming::{
    AnthropicStreamContentBlock,
    AnthropicStreamDelta,
    AnthropicStreamEvent,
};
pub use token_counter::{TokenCounter, estimate_messages_token_count};
pub use traits::{ModelProvider, StreamResult};
pub use types::{
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
    ToolChoice,
    ToolDefinition,
    ToolResult,
    ToolUse,
};
