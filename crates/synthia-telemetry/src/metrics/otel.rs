use std::time::Duration;

use opentelemetry::{
    KeyValue,
    metrics::{Meter, MeterProvider},
};
use opentelemetry_otlp::{MetricExporter, WithExportConfig};
use opentelemetry_sdk::{
    Resource,
    metrics::{PeriodicReader, SdkMeterProvider},
};

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

    /// Record a completed LLM call.
    pub fn record_llm_call(
        &self,
        duration_ms: u64,
        input_tokens: u64,
        output_tokens: u64,
    ) {
        self.llm_call_count.add(1, &[]);
        self.llm_call_duration.record(duration_ms, &[]);

        // Record token usage with attributes
        let input_attrs = [KeyValue::new("type", "input")];
        let output_attrs = [KeyValue::new("type", "output")];
        self.token_usage.add(input_tokens, &input_attrs);
        self.token_usage.add(output_tokens, &output_attrs);
    }

    /// Record a completed tool call.
    pub fn record_tool_call(
        &self,
        tool_name: &str,
        duration_ms: u64,
        success: bool,
    ) {
        let attrs = [
            KeyValue::new("tool.name", tool_name.to_string()),
            KeyValue::new("tool.success", success),
        ];
        self.tool_call_count.add(1, &attrs);
        self.tool_call_duration.record(duration_ms, &attrs);
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

    let reader =
        PeriodicReader::builder(exporter, opentelemetry_sdk::runtime::Tokio)
            .with_interval(Duration::from_secs(30))
            .with_timeout(Duration::from_secs(10))
            .build();

    let resource = Resource::new(vec![opentelemetry::KeyValue::new(
        opentelemetry_semantic_conventions::resource::SERVICE_NAME,
        config.service_name.clone(),
    )]);

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
