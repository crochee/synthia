pub mod agent_metrics;
pub mod compaction_analytics;
pub mod context_trace;
pub mod events;
pub mod metrics;
pub mod sensitive;
pub mod span;
pub mod span_context;
pub mod tracer;

pub use agent_metrics::{
    AgentMetricsConfig,
    AgentMetricsReport,
    EnhancedMetricsCollector,
};
pub use compaction_analytics::{CompactionAnalyticsAttempt, CompactionTrigger};
pub use context_trace::{ApiCallTrace, ContextTracer, compute_prefix_hash};
pub use metrics::{MetricsCollector, MetricsReport};
pub use sensitive::*;
#[cfg(feature = "otel")]
pub use span::SpanAttributesProcessor;
pub use span::{
    SpanBuilder,
    SpanContext as OtSpanContext,
    SpanKind,
    create_compaction_span,
    create_context_assembly_span,
    create_guardian_check_span,
    create_invocation_span,
    create_llm_call_span,
    create_session_span,
    create_step_span,
    create_tool_execution_span,
};
pub use span_context::*;
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
/// - If set: initializes OTLP gRPC tracing pipeline
/// - If not set: falls back to console tracing via tracing_subscriber
///
/// When the `otel` cargo feature is disabled, this always initializes
/// console-only tracing and returns `Ok(TracerInitResult::Console)`.
///
/// # File logging
///
/// When [`tracer::SYNTHIA_LOG_DIR_ENV`] (`SYNTHIA_LOG_DIR`) is set, a file
/// logging layer is composed alongside the console fmt layer, writing to
/// `{SYNTHIA_LOG_DIR}/synthia.log` in append mode with ANSI codes disabled.
///
/// Phase 0 simplification: if both `SYNTHIA_LOG_DIR` and `SYNTHIA_OTLP_ENDPOINT`
/// are set, the file layer wins and OTLP is skipped (the OTLP pipeline calls
/// `try_init` itself and cannot be composed with an extra layer without a
/// Phase 2 refactor). When `SYNTHIA_LOG_DIR` is unset, behavior is exactly as
/// before — no new code path is taken.
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
            // File layer present → console + file (Phase 0: skip OTLP even
            // if configured, to avoid double try_init).
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
            // No file layer → existing behavior (unchanged).
            #[cfg(feature = "otel")]
            {
                init_otlp_tracing(config)
                    .map_err(|e| Error::Telemetry(e.to_string()))
            }
            #[cfg(not(feature = "otel"))]
            {
                init_console_tracing(config)?;
                Ok(TracerInitResult::Console)
            }
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
            Error::Telemetry(format!("Failed to init tracing: {e}"))
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
