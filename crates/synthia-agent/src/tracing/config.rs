//! The [`ObservabilityConfig`] struct + `Default` impl.
//!
//! Mirrors the `[observability]` section in `config.toml`.

/// Configuration for observability features.
///
/// This mirrors the `[observability]` section in `config.toml`.
#[derive(Clone, Debug)]
pub struct ObservabilityConfig {
    /// TCP port for the Prometheus metrics HTTP endpoint.
    pub metrics_port: u16,
    /// Optional OTLP collector endpoint (gRPC).
    pub otlp_endpoint: Option<String>,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            metrics_port: 9090,
            otlp_endpoint: None,
        }
    }
}
