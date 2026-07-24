//! Enhanced Agent metrics with cost tracking, latency percentiles, and quality indicators.

mod collector;
mod types;

#[cfg(test)]
mod tests;

pub use collector::EnhancedMetricsCollector;
pub use types::{AgentMetricsConfig, AgentMetricsReport, LatencyStats};
