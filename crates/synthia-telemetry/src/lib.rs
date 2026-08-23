// Allow `result_large_err` for the whole file: P1b added 4 hidden
// fields to every struct-form variant (frames, backtrace, source,
// and the synthetic source chain), so every `Result<_, Error>` is
// at least 128 bytes. Boxing the error would force every call site
// to `.map_err(|e| *e)` (or accept the allocation), and the existing
// API has no `Box<Error>` in the public surface. Accept the size
// cost; revisit if profiling shows it matters.
#![allow(clippy::result_large_err)]

pub mod metrics;
pub mod propagation;
pub mod tracer;

pub use metrics::{
    HTTP_REQUESTS_DURATION_SECONDS,
    HTTP_REQUESTS_TOTAL,
    TEXT_EXPOSITION_CONTENT_TYPE,
    gather_text,
};
pub use propagation::{
    ExtractedTraceContext,
    InjectedTraceContext,
    TRACEPARENT_HEADER,
    TRACESTATE_HEADER,
    extract_trace_context,
    format_span_id,
    format_trace_id,
    inject_trace_context,
    register_global_propagator,
};
use synthia_core::Error;
pub use tracer::*;
use tracing_subscriber::{
    Layer,
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

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

/// Initialize tracing with automatic OTLP or console fallback.
///
/// Checks `SYNTHIA_OTLP_ENDPOINT` environment variable:
/// - If set: initializes the OTLP pipeline (gRPC or HTTP transport auto-detected
///   from the endpoint URL scheme)
/// - If not set: falls back to console tracing via `tracing_subscriber`
///
/// # File logging
///
/// When [`tracer::SYNTHIA_LOG_DIR_ENV`] (`SYNTHIA_LOG_DIR`) is set, a file
/// logging layer is composed alongside the console fmt layer, writing to
/// `{SYNTHIA_LOG_DIR}/synthia.log` in append mode with ANSI codes disabled.
///
/// Composition limitation: if both `SYNTHIA_LOG_DIR` and `SYNTHIA_OTLP_ENDPOINT`
/// are set, the file layer wins and OTLP is skipped — the OTLP pipeline installs
/// its own subscriber via `try_init`, which cannot be composed with an
/// additional layer in a single global subscriber.
pub fn init_tracing(
    config: &TelemetryConfig,
) -> Result<TracerInitResult, Error> {
    // Try to install a file layer if SYNTHIA_LOG_DIR is set. On failure we
    // warn and proceed without file logging (existing behavior).
    let file_layer: Option<
        Box<dyn Layer<tracing_subscriber::Registry> + Send + Sync>,
    > = std::env::var(tracer::SYNTHIA_LOG_DIR_ENV)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .and_then(|dir| {
            let path = std::path::PathBuf::from(dir);
            match tracer::make_file_layer(&path) {
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
            if std::env::var_os(tracer::SYNTHIA_OTLP_ENDPOINT_ENV).is_some() {
                eprintln!(
                    "Warning: SYNTHIA_LOG_DIR and SYNTHIA_OTLP_ENDPOINT both set; \
                     OTLP tracing skipped (Phase 0 limitation)"
                );
            }
            init_console_with_file(config, fl)?;
            Ok(TracerInitResult::Console)
        }
        None => {
            // No file layer → OTLP pipeline (with console fallback inside).
            init_otlp_tracing(config)
        }
    }
}

/// Initialize console + optional file tracing as a single `try_init` call.
///
/// The file layer MUST be applied directly to `Registry` (before the filter),
/// because `Box<dyn Layer<Registry>>` only implements `Layer<Registry>`, not
/// `Layer<Layered<..., Registry>>`. `EnvFilter` and `fmt::layer()` implement
/// `Layer<S>` for any `S: Subscriber`, so they compose on top.
///
/// The console layer is configured to emit span fields inline (e.g.
/// `trace_id=... span_id=...`), which is the standard stitch point
/// that lets log aggregators (Loki, ELK) correlate log lines with
/// W3C TraceContext traces and with OpenTelemetry spans.
fn init_console_with_file(
    config: &TelemetryConfig,
    file_layer: Box<dyn Layer<tracing_subscriber::Registry> + Send + Sync>,
) -> Result<(), Error> {
    let filter = tracing_subscriber::EnvFilter::try_new(&config.log_level)
        .unwrap_or_else(|_| {
            tracing_subscriber::EnvFilter::default()
                .add_directive("info".parse().unwrap())
        });

    let console_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::NONE);

    tracing_subscriber::registry()
        .with(file_layer)
        .with(filter)
        .with(console_layer)
        .try_init()
        .map_err(|e| {
            Error::telemetry(format!("Failed to init tracing: {e}"))
        })?;

    tracing::info!(
        service = config.service_name,
        "Console + file tracing initialized"
    );
    Ok(())
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
