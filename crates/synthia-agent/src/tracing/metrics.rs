//! 6 metric-recording helpers (counters + histograms).
//!
//! The `metrics` crate macros are no-ops when no global
//! recorder is installed, and properly route to the
//! Prometheus exporter when observability is active. The
//! `INIT_METRICS.call_once` pattern defers registration
//! to a one-time path.
//!
//! - [`record_iteration_metric`] — increments
//!   `synthia_agent_iterations_total`.
//! - [`record_llm_call_metric`] — increments
//!   `synthia_llm_calls_total{provider,model,status}` and
//!   records `synthia_llm_latency_seconds{provider,model}`.
//! - [`record_tool_call_metric`] — increments
//!   `synthia_tool_calls_total{tool_name,status}` and
//!   records `synthia_tool_latency_seconds{tool_name}`.
//! - [`record_hook_error_metric`] — increments
//!   `synthia_hook_errors_total{hook_name}`.
//! - [`record_guardian_block_metric`] — increments
//!   `synthia_guardian_blocks_total{guardrail_type}`.

use super::flags::INIT_METRICS;

/// Increment the iteration counter.
pub fn record_iteration_metric() {
    INIT_METRICS.call_once(|| {});
    metrics::counter!("synthia_agent_iterations_total").increment(1);
}

/// Record an LLM call metric with labels: provider, model, status.
/// Also records latency to the histogram.
pub fn record_llm_call_metric(
    provider: &str,
    model: &str,
    status: &str,
    latency_seconds: f64,
) {
    INIT_METRICS.call_once(|| {});
    metrics::counter!(
        "synthia_llm_calls_total",
        "provider" => provider.to_string(),
        "model" => model.to_string(),
        "status" => status.to_string(),
    )
    .increment(1);
    metrics::histogram!(
        "synthia_llm_latency_seconds",
        "provider" => provider.to_string(),
        "model" => model.to_string(),
    )
    .record(latency_seconds);
}

/// Record a tool call metric with labels: tool_name, status.
/// Also records latency to the histogram.
pub fn record_tool_call_metric(
    tool_name: &str,
    status: &str,
    latency_seconds: f64,
) {
    INIT_METRICS.call_once(|| {});
    metrics::counter!(
        "synthia_tool_calls_total",
        "tool_name" => tool_name.to_string(),
        "status" => status.to_string(),
    )
    .increment(1);
    metrics::histogram!(
        "synthia_tool_latency_seconds",
        "tool_name" => tool_name.to_string(),
    )
    .record(latency_seconds);
}

/// Record a hook error metric with label: hook_name.
pub fn record_hook_error_metric(hook_name: &str) {
    INIT_METRICS.call_once(|| {});
    metrics::counter!(
        "synthia_hook_errors_total",
        "hook_name" => hook_name.to_string(),
    )
    .increment(1);
}

/// Record a guardian block metric with label: guardrail_type.
pub fn record_guardian_block_metric(guardrail_type: &str) {
    INIT_METRICS.call_once(|| {});
    metrics::counter!(
        "synthia_guardian_blocks_total",
        "guardrail_type" => guardrail_type.to_string(),
    )
    .increment(1);
}

/// Record LLM cache token usage for KV cache hit ratio observability.
///
/// Always records `synthia_llm_input_tokens` (the denominator for cache
/// hit ratio). Records `synthia_llm_cache_read_tokens` and
/// `synthia_llm_cache_write_tokens` only when the provider reports them
/// (Anthropic non-streaming path). No-op when no global recorder is
/// installed.
pub fn record_llm_cache_tokens(usage: &synthia_provider::TokenUsage) {
    INIT_METRICS.call_once(|| {});
    metrics::counter!("synthia_llm_input_tokens")
        .increment(usage.prompt_tokens as u64);
    if let Some(read) = usage.cache_read_tokens {
        metrics::counter!("synthia_llm_cache_read_tokens")
            .increment(read as u64);
    }
    if let Some(write) = usage.cache_write_tokens {
        metrics::counter!("synthia_llm_cache_write_tokens")
            .increment(write as u64);
    }

    // Additionally record via OTel SDK when `otel` feature is enabled.
    // This mirrors the pruning engine's pattern (see
    // synthia-context/src/pruning/engine.rs). The `metrics::counter!`
    // facade is a no-op without a Prometheus recorder, so OTel users
    // (SYNTHIA_OTLP_ENDPOINT set) need the direct SDK path to export
    // cache token counters with dot-separated instrument names.
    #[cfg(feature = "otel")]
    {
        let meter = opentelemetry::global::meter("synthia");
        meter
            .u64_counter("synthia.llm.input_tokens")
            .build()
            .add(usage.prompt_tokens as u64, &[]);
        if let Some(read) = usage.cache_read_tokens {
            meter
                .u64_counter("synthia.llm.cache_read_tokens")
                .build()
                .add(read as u64, &[]);
        }
        if let Some(write) = usage.cache_write_tokens {
            meter
                .u64_counter("synthia.llm.cache_write_tokens")
                .build()
                .add(write as u64, &[]);
        }
    }
}
