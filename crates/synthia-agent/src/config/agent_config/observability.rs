//! Serde-compatible observability config that lives inside [`AgentConfig`].
//!
//! Bridges into the in-memory [`ObservabilityConfig`](crate::tracing::ObservabilityConfig)
//! used at runtime. The split is needed because `AgentConfig` must be
//! `Serialize`/`Deserialize` to disk while `ObservabilityConfig` is the
//! runtime type used by the metrics / OTLP layer.

use serde::{Deserialize, Serialize};

use crate::tracing::ObservabilityConfig;

/// Serde-compatible representation of observability config for use in
/// `AgentConfig`. Can be converted to [`ObservabilityConfig`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ObservabilityConfigInner {
    /// TCP port for the Prometheus metrics HTTP endpoint (default: 9090).
    #[serde(default = "default_metrics_port")]
    pub metrics_port: u16,
    /// Optional OTLP collector endpoint (gRPC).
    #[serde(default)]
    pub otlp_endpoint: Option<String>,
}

fn default_metrics_port() -> u16 {
    9090
}

impl From<&ObservabilityConfigInner> for ObservabilityConfig {
    fn from(inner: &ObservabilityConfigInner) -> Self {
        ObservabilityConfig {
            metrics_port: inner.metrics_port,
            otlp_endpoint: inner.otlp_endpoint.clone(),
        }
    }
}
