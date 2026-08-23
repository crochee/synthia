// Allow `result_large_err` for the whole file: P1b added 4 hidden
// fields to every struct-form variant (frames, backtrace, source,
// and the synthetic source chain), so every `Result<_, Error>` is
// at least 128 bytes. Boxing the error would force every call site
// to `.map_err(|e| *e)` (or accept the allocation), and the existing
// API has no `Box<Error>` in the public surface. Accept the size
// cost; revisit if profiling shows it matters.
#![allow(clippy::result_large_err)]

use std::{path::Path, sync::Mutex, time::Duration};

use opentelemetry::{global, trace::TracerProvider};
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::{
    Resource,
    trace::{BatchSpanProcessor, Sampler, SdkTracerProvider},
};
use synthia_core::Error;
use tracing_subscriber::{
    EnvFilter,
    Layer,
    Registry,
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

use crate::{TelemetryConfig, propagation::register_global_propagator};

/// Environment variable for the OTLP collector endpoint.
pub const SYNTHIA_OTLP_ENDPOINT_ENV: &str = "SYNTHIA_OTLP_ENDPOINT";

/// Environment variable for the local log file directory.
///
/// When set, [`crate::init_tracing`] composes a file logging layer
/// (writing to `{SYNTHIA_LOG_DIR}/synthia.log` in append mode, ANSI codes
/// disabled) alongside the console fmt layer.
pub const SYNTHIA_LOG_DIR_ENV: &str = "SYNTHIA_LOG_DIR";

/// Environment variable for the OTel sampler configuration.
///
/// Read at tracer initialization time by [`init_otlp_tracing`]. See
/// [`parse_sampler`] for supported values.
pub const SYNTHIA_OTEL_SAMPLER_ENV: &str = "SYNTHIA_OTEL_SAMPLER";

/// The OTLP transport protocol to use for the span exporter.
///
/// Selected automatically by [`detect_protocol`] based on the endpoint URL
/// scheme and (for `http://`) the gRPC/HTTP standard ports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtlpProtocol {
    /// gRPC exporter backed by tonic.
    ///
    /// Used for `grpc://`, `https://`, no scheme, or `http://` endpoints on the
    /// gRPC standard port 4317 (backward compatible with prior behavior).
    Grpc,
    /// HTTP exporter backed by `opentelemetry-otlp`'s built-in reqwest client.
    ///
    /// Used for `http://` endpoints (except port 4317).
    Http,
}

/// Detect the OTLP protocol to use based on the endpoint URL scheme and port.
///
/// Selection rules (evaluated in priority order):
/// 1. `grpc://` scheme → gRPC (forced, regardless of port).
/// 2. `https://` scheme → gRPC (TLS, backward compatible).
/// 3. `http://` scheme → consult the port for backward compatibility:
///    - port `4317` (gRPC standard) → gRPC.
///    - port `4318` (HTTP standard), any other port, or no port → HTTP.
/// 4. No scheme or any other scheme → gRPC (backward compatible).
pub fn detect_protocol(endpoint: &str) -> OtlpProtocol {
    let trimmed = endpoint.trim();

    if trimmed.starts_with("grpc://") || trimmed.starts_with("https://") {
        return OtlpProtocol::Grpc;
    }

    if let Some(rest) = trimmed.strip_prefix("http://") {
        let authority = rest.split('/').next().unwrap_or(rest);
        if extract_port(authority) == Some(4317) {
            return OtlpProtocol::Grpc;
        }
        return OtlpProtocol::Http;
    }

    // No scheme or unrecognized scheme → gRPC (backward compatible).
    OtlpProtocol::Grpc
}

/// Extract the port from an authority string like `host:port`.
///
/// Returns `None` when no port is present or it is not a valid `u16`.
fn extract_port(authority: &str) -> Option<u16> {
    let port_str = authority.rsplit(':').next()?;
    if !port_str.is_empty() && port_str.bytes().all(|b| b.is_ascii_digit()) {
        port_str.parse().ok()
    } else {
        None
    }
}

/// Parse a sampler spec string into a raw [`Sampler`].
///
/// Supported values:
/// - `always_on` → `Sampler::AlwaysOn`
/// - `always_off` → `Sampler::AlwaysOff`
/// - `trace_id_ratio:<f64>` → `Sampler::TraceIdRatioBased(ratio)`
///   (an unparseable ratio defaults to `1.0` so a typo never silently drops
///   traces)
/// - anything else → `Sampler::AlwaysOn` (safe default that preserves the
///   SDK's out-of-the-box behavior)
///
/// This is the "inner" sampler; [`build_sampler`] wraps it in
/// `Sampler::ParentBased` so a parent trace's sampling decision is honored.
pub fn parse_sampler(spec: &str) -> Sampler {
    let trimmed = spec.trim();
    match trimmed {
        "always_on" => Sampler::AlwaysOn,
        "always_off" => Sampler::AlwaysOff,
        s if s.starts_with("trace_id_ratio:") => {
            let raw = &s["trace_id_ratio:".len()..];
            let ratio: f64 = raw.trim().parse().unwrap_or(1.0);
            Sampler::TraceIdRatioBased(ratio)
        }
        _ => Sampler::AlwaysOn,
    }
}

/// Build the final sampler to install on the tracer provider.
///
/// Wraps the parsed inner sampler in `Sampler::ParentBased` so that the
/// parent trace's sampling decision is honored (matching the SDK default
/// behavior). When `spec` is `None` (env var unset) the inner sampler
/// defaults to `AlwaysOn`, yielding `ParentBased(AlwaysOn)`.
pub fn build_sampler(spec: Option<&str>) -> Sampler {
    let inner = match spec {
        Some(s) => parse_sampler(s),
        None => Sampler::AlwaysOn,
    };
    Sampler::ParentBased(Box::new(inner))
}

/// Result of initializing the tracing pipeline.
pub enum TracerInitResult {
    /// OTLP tracing was successfully initialized.
    Otlp(SdkTracerProvider),
    /// Fell back to console-only tracing (no OTLP endpoint configured).
    Console,
}

/// Initialize the OpenTelemetry OTLP tracing pipeline.
///
/// If `SYNTHIA_OTLP_ENDPOINT` is unset or empty, falls back to console output.
/// Otherwise the OTLP transport protocol (gRPC via tonic or HTTP via reqwest)
/// is auto-selected from the endpoint URL scheme by [`detect_protocol`]:
/// `grpc://` / `https://` / no scheme → gRPC; `http://` → HTTP, except port
/// 4317 which stays gRPC for backward compatibility.
pub fn init_otlp_tracing(
    config: &TelemetryConfig,
) -> Result<TracerInitResult, Error> {
    let endpoint = match std::env::var(SYNTHIA_OTLP_ENDPOINT_ENV) {
        Ok(val) if !val.trim().is_empty() => val.trim().to_string(),
        _ => {
            // No OTLP endpoint configured; fall back to console
            init_console_tracing(config)?;
            return Ok(TracerInitResult::Console);
        }
    };

    let resource = Resource::builder()
        .with_service_name(config.service_name.clone())
        .build();

    let protocol = detect_protocol(&endpoint);
    let exporter = match protocol {
        OtlpProtocol::Grpc => SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint.clone())
            .with_timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| {
                Error::telemetry(format!(
                    "Failed to build OTLP gRPC span exporter: {e}"
                ))
            })?,
        OtlpProtocol::Http => SpanExporter::builder()
            .with_http()
            .with_endpoint(endpoint.clone())
            .with_timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| {
                Error::telemetry(format!(
                    "Failed to build OTLP HTTP span exporter: {e}"
                ))
            })?,
    };

    // Assembly order (per spec Requirement: "装配 MUST 在 exporter 装配之后、
    // provider `build()` 之前"):
    //   resource → sampler → batch_exporter (exporter) → build()
    let sampler_spec = std::env::var(SYNTHIA_OTEL_SAMPLER_ENV).ok();
    let sampler = build_sampler(sampler_spec.as_deref());

    let tracer_provider = SdkTracerProvider::builder()
        .with_resource(resource)
        .with_sampler(sampler)
        .with_span_processor(BatchSpanProcessor::builder(exporter).build())
        .build();

    // Set as the global tracer provider
    global::set_tracer_provider(tracer_provider.clone());

    // Install the W3C TraceContext propagator as the OpenTelemetry global
    // text map propagator. This MUST happen after `set_tracer_provider`
    // but before any spans are produced, so the SDK tracer layer extracts
    // the inbound `traceparent` into the parent span context.
    register_global_propagator();

    let tracer = tracer_provider.tracer(config.service_name.clone());
    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let filter = EnvFilter::try_new(&config.log_level).unwrap_or_else(|_| {
        EnvFilter::default().add_directive("info".parse().unwrap())
    });

    Registry::default()
        .with(filter)
        .with(telemetry_layer)
        .try_init()
        .map_err(|e| {
            Error::telemetry(format!("Failed to init tracing: {e}"))
        })?;

    tracing::info!(
        endpoint = endpoint,
        service = config.service_name,
        protocol = ?protocol,
        sampler = ?sampler_spec.unwrap_or_else(|| "always_on".to_string()),
        "OpenTelemetry OTLP tracing initialized"
    );

    Ok(TracerInitResult::Otlp(tracer_provider))
}

/// Initialize console-based tracing as a fallback.
///
/// Uses `tracing_subscriber` with fmt layer to output traces to stdout.
/// Span fields (notably `trace_id` / `span_id` set by the
/// server's trace-context middleware) are included on every log
/// line so operators can correlate logs with W3C TraceContext
/// traces and metrics without a separate indexing step.
pub fn init_console_tracing(config: &TelemetryConfig) -> Result<(), Error> {
    let filter = EnvFilter::try_new(&config.log_level).unwrap_or_else(|_| {
        EnvFilter::default().add_directive("info".parse().unwrap())
    });

    // Install the W3C propagator even in the console-only path so anything
    // that calls `extract_trace_context` / `inject_trace_context` later
    // receives a real implementation rather than the noop default.
    register_global_propagator();

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

/// Build a file logging layer that writes to `{log_dir}/synthia.log` in
/// append mode. ANSI color codes are disabled to keep the file greppable.
///
/// Returns `Ok(layer)` if the file was successfully opened. The layer is
/// boxed so it can be composed with other layers (e.g. console fmt) by the
/// caller via a single `try_init` call — this avoids the
/// "global subscriber already set" conflict that would arise if file
/// logging called `try_init` separately from `init_otlp_tracing` /
/// `init_console_tracing`.
pub fn make_file_layer(
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

#[cfg(test)]
mod tests {
    use opentelemetry_sdk::trace::Sampler;

    use super::*;

    #[test]
    fn test_parse_sampler_always_on() {
        let s = parse_sampler("always_on");
        assert!(matches!(s, Sampler::AlwaysOn));
    }

    #[test]
    fn test_parse_sampler_always_off() {
        let s = parse_sampler("always_off");
        assert!(matches!(s, Sampler::AlwaysOff));
    }

    #[test]
    fn test_parse_sampler_trace_id_ratio() {
        let s = parse_sampler("trace_id_ratio:0.1");
        if let Sampler::TraceIdRatioBased(r) = s {
            assert!((r - 0.1).abs() < 0.001);
        } else {
            panic!("expected TraceIdRatioBased, got {:?}", s);
        }
    }

    #[test]
    fn test_parse_sampler_invalid_defaults_to_always_on() {
        let s = parse_sampler("garbage");
        assert!(matches!(s, Sampler::AlwaysOn));
    }

    #[test]
    fn test_parse_sampler_invalid_ratio_defaults_to_full() {
        let s = parse_sampler("trace_id_ratio:abc");
        if let Sampler::TraceIdRatioBased(r) = s {
            assert!((r - 1.0).abs() < 0.001);
        } else {
            panic!("expected TraceIdRatioBased");
        }
    }

    #[test]
    fn test_build_sampler_wraps_in_parent_based() {
        let s = build_sampler(Some("always_off"));
        // ParentBased is opaque; verify it's not the raw AlwaysOff.
        assert!(!matches!(s, Sampler::AlwaysOff));
    }

    #[test]
    fn test_build_sampler_default_is_parent_based_always_on() {
        let s = build_sampler(None);
        // Default: ParentBased(AlwaysOn). We can only verify it's not
        // raw AlwaysOn (it's wrapped).
        assert!(!matches!(s, Sampler::AlwaysOn));
    }

    #[test]
    fn test_detect_protocol_grpc_scheme_forces_grpc() {
        assert_eq!(
            detect_protocol("grpc://collector.example:4317"),
            OtlpProtocol::Grpc
        );
        assert_eq!(
            detect_protocol("grpc://collector.example:443"),
            OtlpProtocol::Grpc
        );
    }

    #[test]
    fn test_detect_protocol_https_scheme_uses_grpc() {
        assert_eq!(
            detect_protocol("https://collector.example:443"),
            OtlpProtocol::Grpc
        );
    }

    #[test]
    fn test_detect_protocol_http_4317_uses_grpc() {
        assert_eq!(
            detect_protocol("http://collector.example:4317"),
            OtlpProtocol::Grpc
        );
    }

    #[test]
    fn test_detect_protocol_http_4318_uses_http() {
        assert_eq!(
            detect_protocol("http://collector.example:4318"),
            OtlpProtocol::Http
        );
    }

    #[test]
    fn test_detect_protocol_http_other_port_uses_http() {
        assert_eq!(
            detect_protocol("http://collector.example:8080"),
            OtlpProtocol::Http
        );
    }

    #[test]
    fn test_detect_protocol_no_scheme_defaults_to_grpc() {
        assert_eq!(
            detect_protocol("collector.example:4317"),
            OtlpProtocol::Grpc
        );
    }
}
