//! [`synthia_telemetry`] — logging, traces, and
//! metrics.
//!
//! [`init_tracing`] installs the subscriber (console, OTLP, or both);
//! `tracer` carries the span helpers the agent and tool paths
//! annotate themselves with; `metrics` holds the Prometheus
//! instruments and the text exposition the server's `/metrics` route
//! serves; `propagation` moves W3C `traceparent` / `tracestate`
//! across process boundaries.

pub use synthia_telemetry::*;
