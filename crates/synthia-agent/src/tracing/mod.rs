//! Observability module — tracing spans and Prometheus
//! metrics for the Synthia agent loop.
//!
//! # Public API
//!
//! - [`MetricsServer`] — Prometheus scrape endpoint (real
//!   with the `observability` feature, stub otherwise).
//! - [`ObservabilityConfig`] — the `[observability]`
//!   section in `config.toml`.
//!
//! # Helper Functions (crate-internal)
//!
//! 3 tracing-span helpers in [`spans`] and 6 metric
//! recorders in [`metrics`] are reachable through this
//! module index for unit tests and ad-hoc callers.
//!
//! # Module Layout
//!
//! - [`config`]: [`config::ObservabilityConfig`] struct
//!   + `Default` (port = 9090, no OTLP).
//! - [`flags`]: [`flags::RECORDER_INSTALLED`] (atomic
//!   bool) + `INIT_METRICS` (Once) +
//!   [`flags::is_recorder_installed`] accessor.
//! - [`spans`]: 3 tracing-span helpers
//!   ([`spans::trace_iteration`],
//!   [`spans::trace_llm_call`],
//!   [`spans::trace_tool_call`]).
//! - [`metrics`]: 6 metric recorders
//!   ([`metrics::record_iteration_metric`],
//!   `record_llm_call_metric`, `record_tool_call_metric`,
//!   `record_hook_error_metric`,
//!   `record_guardian_block_metric`).
//! - [`server`]: [`server::MetricsServer`] (real with
//!   `observability` feature; stub without).
//! - [`tests`]: 16 unit tests.

mod config;
mod flags;
mod metrics;
mod server;
mod spans;

#[cfg(test)]
mod tests;

pub use config::ObservabilityConfig;
pub use flags::is_recorder_installed;
pub use metrics::{
    record_guardian_block_metric,
    record_hook_error_metric,
    record_iteration_metric,
    record_llm_cache_tokens,
    record_llm_call_metric,
    record_tool_call_metric,
};
pub use server::MetricsServer;
pub use spans::{trace_iteration, trace_llm_call, trace_tool_call};
