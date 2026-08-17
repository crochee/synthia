#[cfg(feature = "otel")]
pub mod names;

#[cfg(feature = "otel")]
mod otel;

#[cfg(feature = "otel")]
pub use otel::{TelemetryMetrics, init_metrics};
