//! 3 tracing-span helpers.
//!
//! All three return a [`tracing::Span`] which the caller
//! `.enter()`s for the duration of the operation. The
//! span's name + attributes feed into `tracing-subscriber`
//! for log/OTLP output.
//!
//! - [`trace_iteration`] — `agent.iteration` span with
//!   `iteration_number`, `llm_provider`, `model_name`,
//!   `token_count`, `tool_calls_count`, `duration_ms`.
//! - [`trace_llm_call`] — `llm.call` span with `provider`,
//!   `model`, `input_tokens`, `output_tokens`,
//!   `latency_ms`, `status`.
//! - [`trace_tool_call`] — `tool.execute` span with
//!   `tool_name`, `args` (truncated to 500 chars),
//!   `status`, `duration_ms`.

/// Emit a span named `agent.iteration` with attributes:
/// `iteration_number`, `llm_provider`, `model_name`, `token_count`,
/// `tool_calls_count`, `duration_ms`.
///
/// The span is returned so the caller can `.enter()` it for the duration
/// of the iteration.
pub fn trace_iteration(
    iteration_number: usize,
    llm_provider: &str,
    model_name: &str,
    token_count: usize,
    tool_calls_count: usize,
    duration_ms: u64,
) -> tracing::Span {
    tracing::info_span!(
        "agent.iteration",
        iteration_number = iteration_number,
        llm_provider = llm_provider,
        model_name = model_name,
        token_count = token_count,
        tool_calls_count = tool_calls_count,
        duration_ms = duration_ms,
    )
}

/// Emit a span named `llm.call` with attributes:
/// `provider`, `model`, `input_tokens`, `output_tokens`, `latency_ms`, `status`.
pub fn trace_llm_call(
    provider: &str,
    model: &str,
    input_tokens: usize,
    output_tokens: usize,
    latency_ms: u64,
    status: &str,
) -> tracing::Span {
    tracing::info_span!(
        "llm.call",
        provider = provider,
        model = model,
        input_tokens = input_tokens,
        output_tokens = output_tokens,
        latency_ms = latency_ms,
        status = status,
    )
}

/// Emit a span named `tool.execute` with attributes:
/// `tool_name`, `args` (truncated to 500 chars), `status`, `duration_ms`.
pub fn trace_tool_call(
    tool_name: &str,
    args: &str,
    status: &str,
    duration_ms: u64,
) -> tracing::Span {
    let truncated = if args.len() > 500 { &args[..500] } else { args };
    tracing::info_span!(
        "tool.execute",
        tool_name = tool_name,
        args = truncated,
        status = status,
        duration_ms = duration_ms,
    )
}
