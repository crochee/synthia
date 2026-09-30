//! Console (and optional file) tracing — the half of
//! [`crate::init_tracing`] that is always available.
//!
//! Without the `otlp` feature this module is the *whole* pipeline: a
//! consumer that wants structured logs pays for `tracing-subscriber`
//! and nothing else — no OpenTelemetry SDK, no exporter, no HTTP
//! client.

use std::{path::Path, sync::Mutex};

use synthia_core::Error;
use tracing_subscriber::{
    Layer,
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

use crate::TelemetryConfig;

/// Environment variable for the local log file directory.
///
/// When set, [`crate::init_tracing`] composes a file logging layer
/// (writing to `{SYNTHIA_LOG_DIR}/synthia.log` in append mode, ANSI
/// codes disabled) alongside the console fmt layer.
pub const SYNTHIA_LOG_DIR_ENV: &str = "SYNTHIA_LOG_DIR";

/// Initialize console-based tracing.
///
/// Uses `tracing_subscriber` with fmt layer to output traces to stdout.
/// Span fields (notably `trace_id` / `span_id` set by the server's
/// trace-context middleware) are included on every log line so
/// operators can correlate logs with W3C TraceContext traces and
/// metrics without a separate indexing step.
pub(crate) fn init_console_tracing(
    config: &TelemetryConfig,
) -> Result<(), Error> {
    let filter = env_filter(config);

    // With the `otlp` feature the W3C propagator is installed here so
    // that anything calling `extract_trace_context` /
    // `inject_trace_context` later receives a real implementation
    // rather than the OTel no-op default. Without the feature there is
    // no propagator module to install, and nothing to correlate with.
    #[cfg(feature = "otlp")]
    crate::propagation::register_global_propagator();

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .try_init()
        .map_err(|e| {
            Error::telemetry(format!("Failed to init console tracing: {e}"))
        })?;

    tracing::info!(
        service = config.service_name,
        "Console tracing initialized (OTLP not configured)"
    );

    Ok(())
}

/// Initialize console + optional file tracing as a single `try_init`
/// call.
///
/// The file layer MUST be applied directly to `Registry` (before the
/// filter), because `Box<dyn Layer<Registry>>` only implements
/// `Layer<Registry>`, not `Layer<Layered<..., Registry>>`. `EnvFilter`
/// and `fmt::layer()` implement `Layer<S>` for any `S: Subscriber`, so
/// they compose on top.
///
/// The console layer is configured to emit span fields inline (e.g.
/// `trace_id=... span_id=...`), which is the standard stitch point
/// that lets log aggregators (Loki, ELK) correlate log lines with W3C
/// TraceContext traces and with OpenTelemetry spans.
pub(crate) fn init_console_with_file(
    config: &TelemetryConfig,
    file_layer: Box<dyn Layer<tracing_subscriber::Registry> + Send + Sync>,
) -> Result<(), Error> {
    let filter = env_filter(config);

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

/// Build a file logging layer that writes to `{log_dir}/synthia.log` in
/// append mode. ANSI color codes are disabled to keep the file
/// greppable.
///
/// Returns `Ok(layer)` if the file was successfully opened. The layer
/// is boxed so it can be composed with other layers (e.g. console fmt)
/// by the caller via a single `try_init` call — this avoids the
/// "global subscriber already set" conflict that would arise if file
/// logging called `try_init` separately from `init_otlp_tracing` /
/// `init_console_tracing`.
pub(crate) fn make_file_layer(
    log_dir: &Path,
) -> Result<Box<dyn Layer<tracing_subscriber::Registry> + Send + Sync>, Error> {
    std::fs::create_dir_all(log_dir).map_err(|e| {
        Error::telemetry(format!("Failed to create log dir: {e}"))
    })?;

    let log_path = log_dir.join("synthia.log");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| {
            Error::telemetry(format!("Failed to open log file: {e}"))
        })?;

    // `std::fs::File` does not implement `MakeWriter` directly in
    // tracing-subscriber 0.3; `Mutex<File>` does and is `Send + Sync`.
    let layer = tracing_subscriber::fmt::layer()
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .with_target(true)
        .with_level(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .boxed();

    Ok(layer)
}

/// Build the `EnvFilter` shared by every console path: the configured
/// level, falling back to `info` when the string is not a valid
/// directive (a typo in `RUST_LOG` must not lose the logs entirely).
fn env_filter(config: &TelemetryConfig) -> tracing_subscriber::EnvFilter {
    tracing_subscriber::EnvFilter::try_new(&config.log_level).unwrap_or_else(
        |_| {
            tracing_subscriber::EnvFilter::default()
                .add_directive("info".parse().unwrap())
        },
    )
}
