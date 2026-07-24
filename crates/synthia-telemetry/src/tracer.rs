#[cfg(feature = "otel")]
use std::time::Duration;
use std::{path::Path, sync::Mutex};

#[cfg(feature = "otel")]
use opentelemetry::{global, trace::TracerProvider};
#[cfg(feature = "otel")]
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
#[cfg(feature = "otel")]
use opentelemetry_sdk::{
    Resource,
    trace::{Sampler, TracerProvider as SdkTracerProvider},
};
use synthia_core::Error;
#[cfg(feature = "otel")]
use tracing_subscriber::Registry;
use tracing_subscriber::{
    EnvFilter,
    Layer,
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

use crate::TelemetryConfig;
#[cfg(feature = "otel")]
use crate::span::attributes_processor::SpanAttributesProcessor;

/// Environment variable for the OTLP collector endpoint.
pub const SYNTHIA_OTLP_ENDPOINT_ENV: &str = "SYNTHIA_OTLP_ENDPOINT";

/// Environment variable for the local log file directory.
///
/// When set, [`init_tracing`] composes a file logging layer (writing to
/// `{SYNTHIA_LOG_DIR}/synthia.log` in append mode, ANSI codes disabled)
/// alongside the console fmt layer. File logging is independent of the
/// `otel` cargo feature.
pub const SYNTHIA_LOG_DIR_ENV: &str = "SYNTHIA_LOG_DIR";

/// Environment variable for the OTel sampler configuration.
///
/// Read at tracer initialization time by [`init_otlp_tracing`]. See
/// [`parse_sampler`] for supported values.
#[cfg(feature = "otel")]
pub const SYNTHIA_OTEL_SAMPLER_ENV: &str = "SYNTHIA_OTEL_SAMPLER";

/// The OTLP transport protocol to use for the span exporter.
///
/// Selected automatically by [`detect_protocol`] based on the endpoint URL
/// scheme and (for `http://`) the gRPC/HTTP standard ports.
#[cfg(feature = "otel")]
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
#[cfg(feature = "otel")]
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
#[cfg(feature = "otel")]
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
#[cfg(feature = "otel")]
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
#[cfg(feature = "otel")]
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
    #[cfg(feature = "otel")]
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
#[cfg(feature = "otel")]
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

    let resource = Resource::new(vec![opentelemetry::KeyValue::new(
        opentelemetry_semantic_conventions::resource::SERVICE_NAME,
        config.service_name.clone(),
    )]);

    let protocol = detect_protocol(&endpoint);
    let exporter = match protocol {
        OtlpProtocol::Grpc => SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint.clone())
            .with_timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| {
                Error::Telemetry(format!(
                    "Failed to build OTLP gRPC span exporter: {e}"
                ))
            })?,
        OtlpProtocol::Http => SpanExporter::builder()
            .with_http()
            .with_endpoint(endpoint.clone())
            .with_timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| {
                Error::Telemetry(format!(
                    "Failed to build OTLP HTTP span exporter: {e}"
                ))
            })?,
    };

    // Assembly order (per spec Requirement: "装配 MUST 在 exporter 装配之后、
    // provider `build()` 之前"):
    //   resource → sampler → batch_exporter (exporter) → span_processor → build()
    // `with_batch_exporter` internally wraps the exporter in a
    // `BatchSpanProcessor` and registers it; `with_span_processor` then
    // appends `SpanAttributesProcessor` to the same processor list. Both
    // processors run on every span: `SpanAttributesProcessor::on_start`
    // injects the 6 standard attributes, and the batch processor handles
    // async export on `on_end`. They do not conflict.
    let sampler_spec = std::env::var(SYNTHIA_OTEL_SAMPLER_ENV).ok();
    let sampler = build_sampler(sampler_spec.as_deref());

    let tracer_provider = SdkTracerProvider::builder()
        .with_resource(resource)
        .with_sampler(sampler)
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_span_processor(SpanAttributesProcessor::new())
        .build();

    // Set as the global tracer provider
    global::set_tracer_provider(tracer_provider.clone());

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
            Error::Telemetry(format!("Failed to init tracing: {e}"))
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
pub fn init_console_tracing(config: &TelemetryConfig) -> Result<(), Error> {
    let filter = EnvFilter::try_new(&config.log_level).unwrap_or_else(|_| {
        EnvFilter::default().add_directive("info".parse().unwrap())
    });

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .try_init()
        .map_err(|e| {
            Error::Telemetry(format!("Failed to init console tracing: {e}"))
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
///
/// File logging is independent of the `otel` cargo feature.
pub fn make_file_layer(
    log_dir: &Path,
) -> Result<Box<dyn Layer<tracing_subscriber::Registry> + Send + Sync>, Error> {
    std::fs::create_dir_all(log_dir).map_err(|e| {
        Error::Telemetry(format!("Failed to create log dir: {e}"))
    })?;

    let log_path = log_dir.join("synthia.log");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| {
            Error::Telemetry(format!("Failed to open log file: {e}"))
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

/// Initialize file-based logging to `{log_dir}/synthia.log`.
///
/// Convenience wrapper that builds a file layer via [`make_file_layer`] and
/// installs it as the global subscriber with `try_init`. Intended for tests
/// and standalone tools. Production code should call [`make_file_layer`]
/// directly and compose the layer with other layers in [`init_tracing`].
///
/// Note: `try_init` sets a process-wide global subscriber; calling this
/// function twice (or after another `init_*_tracing` call) in the same
/// process returns an error.
pub fn init_file_logging(log_dir: &Path) -> Result<(), Error> {
    let file_layer = make_file_layer(log_dir)?;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    // `file_layer` is `Box<dyn Layer<Registry>>`, so it MUST be applied
    // directly to `Registry` (before `filter`), because `Box<dyn Layer<S>>`
    // only implements `Layer<S>`, not `Layer<Layered<..., S>>`.
    tracing_subscriber::registry()
        .with(file_layer)
        .with(filter)
        .try_init()
        .map_err(|e| {
            Error::Telemetry(format!("Failed to init file logging: {e}"))
        })?;

    tracing::info!(log_dir = ?log_dir, "File logging initialized");
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "otel")]
    mod otel {
        use opentelemetry_sdk::trace::Sampler;

        use super::super::*;

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
}
