//! # synthia-telemetry
//!
//! Observability, in two independently switchable halves:
//!
//! | Half | Feature | What it brings |
//! |---|---|---|
//! | console + file logging | *always on* | `tracing` / `tracing-subscriber` |
//! | Prometheus vectors | `metrics` | `prometheus` + `lazy_static` |
//! | OTLP trace export | `otlp` | the `opentelemetry*` family, `tracing-opentelemetry`, gRPC and HTTP transports |
//!
//! `default = ["metrics", "otlp"]` — the full stack, which is what a
//! deployment (`synthia-server`) wants. A consumer that only needs the
//! tool metrics from [`metrics`] takes
//! `synthia-telemetry = { version = "0.1", default-features = false, features = ["metrics"] }`
//! and never compiles an exporter; a consumer that only wants logs
//! takes `default-features = false` and gets
//! [`init_tracing`] → [`TracingInit::Console`].
//!
//! The `otlp` feature gates [`propagation`] and [`tracer`]
//! together: W3C `traceparent` extraction only means something when
//! there is an SDK to hand the span context to.
//!
//! See the crate README for the environment variables
//! (`SYNTHIA_OTLP_ENDPOINT`, `SYNTHIA_OTEL_SAMPLER`,
//! `SYNTHIA_LOG_DIR`) and the protocol-selection rules.

// Allow `result_large_err` for the whole file: P1b added 4 hidden
// fields to every struct-form variant (frames, backtrace, source,
// and the synthetic source chain), so every `Result<_, Error>` is
// at least 128 bytes. Boxing the error would force every call site
// to `.map_err(|e| *e)` (or accept the allocation), and the existing
// API has no `Box<Error>` in the public surface. Accept the size
// cost; revisit if profiling shows it matters.
#![allow(clippy::result_large_err)]

pub mod console;
#[cfg(feature = "metrics")]
pub mod metrics;
#[cfg(feature = "otlp")]
pub mod propagation;
#[cfg(feature = "otlp")]
pub mod tracer;

// Only the two env-var names and `TracingInit` are the crate's
// public entry contract here; the tracer module's protocol and
// sampler helpers are internal plumbing reached through
// `init_tracing`.
pub use console::SYNTHIA_LOG_DIR_ENV;
#[cfg(feature = "metrics")]
pub use metrics::{
    HTTP_REQUESTS_DURATION_SECONDS,
    HTTP_REQUESTS_TOTAL,
    TEXT_EXPOSITION_CONTENT_TYPE,
    gather_text,
    record_tool_duration,
    record_tool_outcome_metric,
    record_tool_panic,
    record_tool_truncation,
};
#[cfg(feature = "otlp")]
pub use propagation::{
    ExtractedTraceContext,
    InjectedTraceContext,
    TRACEPARENT_HEADER,
    TRACESTATE_HEADER,
    extract_trace_context,
    format_span_id,
    format_trace_id,
    inject_trace_context,
};
use synthia_core::Error;
#[cfg(feature = "otlp")]
pub use tracer::{SYNTHIA_OTEL_SAMPLER_ENV, SYNTHIA_OTLP_ENDPOINT_ENV};

#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    pub service_name: String,
    pub log_level: String,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            service_name: "synthia".to_string(),
            log_level: "info".to_string(),
        }
    }
}

/// What [`init_tracing`] installed.
///
/// The `Otlp` variant exists only with the `otlp` feature; without it
/// the enum is a single-variant [`TracingInit::Console`], so a caller
/// that matches on it keeps compiling whichever way the crate was
/// built (the exporter's `SdkTracerProvider` is only nameable when it
/// was compiled in).
pub enum TracingInit {
    /// The OTLP pipeline is installed. The provider is returned so the
    /// caller can keep it alive for the process's lifetime and
    /// `shutdown()` it on exit to flush pending spans.
    #[cfg(feature = "otlp")]
    Otlp(opentelemetry_sdk::trace::SdkTracerProvider),
    /// Console tracing only — either no OTLP endpoint was configured or
    /// the crate was built without the `otlp` feature.
    Console,
}

/// Initialize tracing: console (plus an optional file layer) always,
/// OTLP when it is configured *and* compiled in.
///
/// ## OTLP selection
///
/// With the `otlp` feature, `SYNTHIA_OTLP_ENDPOINT` decides:
/// - set: installs the OTLP pipeline (gRPC or HTTP transport
///   auto-detected from the endpoint URL scheme);
/// - unset: falls back to console tracing.
///
/// Without the feature the OTLP branch does not exist; setting
/// `SYNTHIA_OTLP_ENDPOINT` has no effect.
///
/// ## File logging
///
/// When `SYNTHIA_LOG_DIR` is set, a file logging layer is composed
/// alongside the console fmt layer, writing to
/// `{SYNTHIA_LOG_DIR}/synthia.log` in append mode with ANSI codes
/// disabled.
///
/// Composition limitation: if both `SYNTHIA_LOG_DIR` and
/// `SYNTHIA_OTLP_ENDPOINT` are set, the file layer wins and OTLP is
/// skipped — the OTLP pipeline installs its own subscriber via
/// `try_init`, which cannot be composed with an additional layer in a
/// single global subscriber.
pub fn init_tracing(config: &TelemetryConfig) -> Result<TracingInit, Error> {
    // Try to install a file layer if SYNTHIA_LOG_DIR is set. On failure we
    // warn and proceed without file logging (existing behavior).
    let file_layer: Option<
        Box<
            dyn tracing_subscriber::Layer<tracing_subscriber::Registry>
                + Send
                + Sync,
        >,
    > = std::env::var(SYNTHIA_LOG_DIR_ENV)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .and_then(|dir| {
            let path = std::path::PathBuf::from(dir);
            match console::make_file_layer(&path) {
                Ok(layer) => Some(layer),
                Err(e) => {
                    eprintln!("Warning: file logging init failed: {}", e);
                    None
                }
            }
        });

    match file_layer {
        Some(fl) => {
            // File layer present → console + file. The OTLP pipeline can't
            // share a global subscriber, so we skip it even if configured.
            #[cfg(feature = "otlp")]
            if std::env::var_os(tracer::SYNTHIA_OTLP_ENDPOINT_ENV).is_some() {
                eprintln!(
                    "Warning: SYNTHIA_LOG_DIR and SYNTHIA_OTLP_ENDPOINT both set; \
                     OTLP tracing skipped (Phase 0 limitation)"
                );
            }
            console::init_console_with_file(config, fl)?;
            Ok(TracingInit::Console)
        }
        // No file layer → the OTLP pipeline when it is compiled in,
        // console otherwise.
        None => init_without_file_layer(config),
    }
}

/// The no-file-layer path: OTLP when compiled in, console when not.
#[cfg(feature = "otlp")]
fn init_without_file_layer(
    config: &TelemetryConfig,
) -> Result<TracingInit, Error> {
    tracer::init_otlp_tracing(config)
}

/// The no-file-layer path without the `otlp` feature: console only.
#[cfg(not(feature = "otlp"))]
fn init_without_file_layer(
    config: &TelemetryConfig,
) -> Result<TracingInit, Error> {
    console::init_console_tracing(config)?;
    Ok(TracingInit::Console)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_config_default() {
        let config = TelemetryConfig::default();
        assert_eq!(config.service_name, "synthia");
        assert_eq!(config.log_level, "info");
    }
}
