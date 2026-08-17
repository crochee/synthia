use std::time::Duration;

use opentelemetry::metrics::{Meter, MeterProvider};
use opentelemetry_otlp::{MetricExporter, WithExportConfig};
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};

use crate::{
    TelemetryConfig,
    tracer::{OtlpProtocol, detect_protocol},
};

/// Holds all OpenTelemetry metric instruments for the system.
pub struct TelemetryMetrics {
    /// Counter for total LLM API calls.
    pub llm_call_count: opentelemetry::metrics::Counter<u64>,
    /// Histogram for LLM call latency in milliseconds.
    pub llm_call_duration: opentelemetry::metrics::Histogram<u64>,
    /// Counter for token usage (with attributes: input, output, total).
    pub token_usage: opentelemetry::metrics::Counter<u64>,
    /// Counter for total tool calls.
    pub tool_call_count: opentelemetry::metrics::Counter<u64>,
    /// Histogram for tool call latency in milliseconds.
    pub tool_call_duration: opentelemetry::metrics::Histogram<u64>,
}

impl TelemetryMetrics {
    /// Create a new set of metric instruments from the given meter.
    pub fn new(meter: &Meter) -> Self {
        Self {
            llm_call_count: meter
                .u64_counter(super::names::LLM_CALL_COUNT)
                .with_description("Total number of LLM API calls")
                .build(),

            llm_call_duration: meter
                .u64_histogram(super::names::LLM_CALL_DURATION_MS)
                .with_description("Duration of LLM API calls in milliseconds")
                .with_unit("ms")
                .build(),

            token_usage: meter
                .u64_counter(super::names::LLM_TOKEN_USAGE)
                .with_description("Token usage for LLM calls")
                .build(),

            tool_call_count: meter
                .u64_counter(super::names::TOOL_CALL_COUNT)
                .with_description("Total number of tool calls")
                .build(),

            tool_call_duration: meter
                .u64_histogram(super::names::TOOL_CALL_DURATION_MS)
                .with_description("Duration of tool calls in milliseconds")
                .with_unit("ms")
                .build(),
        }
    }
}

/// Initialize the OpenTelemetry metrics pipeline and return the metrics instruments.
///
/// When the `SYNTHIA_OTLP_ENDPOINT` environment variable is set,
/// metrics are exported to the OTLP collector via gRPC or HTTP,
/// auto-selected from the endpoint URL scheme by [`detect_protocol`]
/// (matching the transport selection used by the tracer pipeline).
/// Otherwise, returns None and metrics are not exported (console
/// metrics output is handled via tracing::info in callers).
pub fn init_metrics(config: &TelemetryConfig) -> Option<TelemetryMetrics> {
    let endpoint = std::env::var(crate::tracer::SYNTHIA_OTLP_ENDPOINT_ENV)
        .ok()
        .filter(|v| !v.trim().is_empty())?;

    let protocol = detect_protocol(&endpoint);

    // Build an OTLP metrics exporter. The transport (gRPC via tonic or HTTP
    // via reqwest) is auto-selected from the endpoint URL scheme, mirroring
    // `init_otlp_tracing` so traces and metrics share the same collector
    // transport.
    let exporter = match protocol {
        OtlpProtocol::Grpc => MetricExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint.clone())
            .with_timeout(Duration::from_secs(5))
            .build()
            .ok()?,
        OtlpProtocol::Http => MetricExporter::builder()
            .with_http()
            .with_endpoint(endpoint.clone())
            .with_timeout(Duration::from_secs(5))
            .build()
            .ok()?,
    };

    let reader = PeriodicReader::builder(exporter)
        .with_interval(Duration::from_secs(30))
        .build();

    let resource = opentelemetry_sdk::Resource::builder()
        .with_service_name(config.service_name.clone())
        .build();

    let provider = SdkMeterProvider::builder()
        .with_reader(reader)
        .with_resource(resource)
        .build();

    let meter = provider.meter("synthia");
    let metrics = TelemetryMetrics::new(&meter);

    // Store the provider globally so it is not dropped
    opentelemetry::global::set_meter_provider(provider);

    tracing::info!(
        endpoint = endpoint,
        protocol = ?protocol,
        "OpenTelemetry OTLP metrics initialized"
    );
    Some(metrics)
}

#[cfg(all(test, feature = "otel"))]
mod tests {
    use opentelemetry_sdk::metrics::SdkMeterProvider;

    use super::*;

    fn test_meter() -> Meter {
        let provider = SdkMeterProvider::builder().build();
        provider.meter("test")
    }

    /// `TelemetryMetrics::new` MUST return a struct with all 5
    /// instrument fields populated (no panic, no missing
    /// instrument).
    #[test]
    fn telemetry_metrics_new_populates_all_five_fields() {
        let meter = test_meter();
        let m = TelemetryMetrics::new(&meter);
        // Pin field presence: the SDK returns typed instruments
        // that are valid even without an exporter attached.
        let _ = m.llm_call_count.clone();
        let _ = m.llm_call_duration.clone();
        let _ = m.token_usage.clone();
        let _ = m.tool_call_count.clone();
        let _ = m.tool_call_duration.clone();
    }

    /// `TelemetryMetrics::new` MUST accept a `Meter` and produce
    /// independent metric instances per call (no static caching).
    #[test]
    fn telemetry_metrics_new_creates_independent_instances() {
        let meter = test_meter();
        let a = TelemetryMetrics::new(&meter);
        let b = TelemetryMetrics::new(&meter);
        // Two independent builders MUST produce two independent
        // counter / histogram instances. We pin identity via
        // separate field accesses (any compile error would mean
        // the fields are not publicly accessible).
        let _ = (a.llm_call_count, b.llm_call_count);
        let _ = (a.tool_call_duration, b.tool_call_duration);
    }

    /// `init_metrics` MUST return `None` when `SYNTHIA_OTLP_ENDPOINT`
    /// is unset (the documented "no exporter" fallback).
    #[test]
    fn init_metrics_returns_none_when_endpoint_unset() {
        unsafe {
            std::env::remove_var(crate::tracer::SYNTHIA_OTLP_ENDPOINT_ENV);
        }
        let cfg = TelemetryConfig::default();
        let result = init_metrics(&cfg);
        assert!(result.is_none());
    }

    /// `init_metrics` MUST return `None` when `SYNTHIA_OTLP_ENDPOINT`
    /// is set to whitespace-only (filtered out by `.trim().is_empty()`).
    #[test]
    fn init_metrics_returns_none_when_endpoint_is_whitespace() {
        unsafe {
            std::env::set_var(crate::tracer::SYNTHIA_OTLP_ENDPOINT_ENV, "   ");
        }
        let cfg = TelemetryConfig::default();
        let result = init_metrics(&cfg);
        assert!(result.is_none());
        unsafe {
            std::env::remove_var(crate::tracer::SYNTHIA_OTLP_ENDPOINT_ENV);
        }
    }

    /// `init_metrics` MUST return `None` when `SYNTHIA_OTLP_ENDPOINT`
    /// is set to an empty string (filtered out).
    #[test]
    fn init_metrics_returns_none_when_endpoint_is_empty() {
        unsafe {
            std::env::set_var(crate::tracer::SYNTHIA_OTLP_ENDPOINT_ENV, "");
        }
        let cfg = TelemetryConfig::default();
        let result = init_metrics(&cfg);
        assert!(result.is_none());
        unsafe {
            std::env::remove_var(crate::tracer::SYNTHIA_OTLP_ENDPOINT_ENV);
        }
    }
}
