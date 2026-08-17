/// Metric names used by the OTel metric pipeline.
#[cfg(feature = "otel")]
pub const LLM_CALL_COUNT: &str = "synthia.llm.call_count";
#[cfg(feature = "otel")]
pub const LLM_CALL_DURATION_MS: &str = "synthia.llm.call_duration_ms";
#[cfg(feature = "otel")]
pub const LLM_TOKEN_USAGE: &str = "synthia.llm.token_usage";
#[cfg(feature = "otel")]
pub const TOOL_CALL_COUNT: &str = "synthia.tool.call_count";
#[cfg(feature = "otel")]
pub const TOOL_CALL_DURATION_MS: &str = "synthia.tool.call_duration_ms";
