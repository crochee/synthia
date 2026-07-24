mod collector;
pub mod names;
#[cfg(feature = "otel")]
mod otel;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use collector::{MetricsCollector, MetricsReport};
#[cfg(feature = "otel")]
pub use otel::{TelemetryMetrics, init_metrics};
