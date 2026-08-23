# synthia-telemetry

Observability primitives for the synthia agent framework: tracing setup,
context-trace recording, Prometheus RED metrics, and OpenTelemetry (OTel)
integration.

This crate exposes **all** of its dependencies unconditionally — there are
no cargo features for compile-isolation in `synthia-telemetry` itself.
The crate is intentionally small and self-contained; every consumer pulls
the full observability stack and decides at the call site whether to
invoke the OTLP exporter, register the metrics middleware, etc.

## OpenTelemetry (OTel)

OTel core + OTLP exporter + W3C TraceContext propagator + `SpanAttributesProcessor`
are always-on. Drop in the tracer via [`init_tracing`]; if
`SYNTHIA_OTLP_ENDPOINT` is unset, it falls back to console output.

```rust
use synthia_telemetry::{init_tracing, TelemetryConfig, TracerInitResult};

let config = TelemetryConfig::default();
match init_tracing(&config)? {
    TracerInitResult::Otlp(provider) => {
        // spans are exported to the OTLP collector; keep `provider`
        // alive for the duration of the process and call
        // `provider.shutdown()` on exit to flush pending spans
    }
    TracerInitResult::Console => {
        // tracing-subscriber fmt layer writes to stdout
    }
}
```

Downstream crates do not need to opt into OTel — calling
`init_tracing(&config)` is enough to start the OTLP pipeline. The
W3C TraceContext propagator is registered automatically on every
call so `traceparent` / `tracestate` headers are honored out of the box.

## Prometheus metrics

The static [`HTTP_REQUESTS_TOTAL`](https://docs.rs/synthia-telemetry/) and
[`HTTP_REQUESTS_DURATION_SECONDS`](https://docs.rs/synthia-telemetry/) RED
vectors are always-on. They are labeled by `(method, matched_path)` where
`matched_path` is the axum route template, so a parameterized path like
`/api/v1/chat/sessions/{id}/messages` collapses to a single time series
regardless of how many distinct session ids are queried.

`synthia-server` mounts the per-request `track_metrics` middleware
and exposes a public `/metrics` endpoint unconditionally. The vectors
in this crate remain compiled and reachable whether or not anything
currently labels a child sample.

[`gather_text`] returns the standard Prometheus text exposition body
(version 0.0.4) suitable for serving from the endpoint.

### OTLP Endpoint Configuration

Set the `SYNTHIA_OTLP_ENDPOINT` environment variable to point at an OTLP
collector. The transport protocol (gRPC via tonic, or HTTP via reqwest) is
auto-selected from the URL scheme by [`detect_protocol`]:

| Scheme              | Port         | Protocol       | Example                               |
|---------------------|--------------|----------------|---------------------------------------|
| `grpc://`           | any          | gRPC (tonic)   | `grpc://localhost:4317`               |
| `https://`          | any          | gRPC (TLS)     | `https://collector.example.com:4317`  |
| `http://`           | `4317`       | gRPC           | `http://localhost:4317` (backward compat) |
| `http://`           | `4318`/other | HTTP (reqwest) | `http://localhost:4318`               |
| none / other        | any          | gRPC           | `localhost:4317`                      |

If `SYNTHIA_OTLP_ENDPOINT` is unset or empty, [`init_tracing`] falls back
to console tracing and returns [`TracerInitResult::Console`].

### Sampler Configuration

The tracer provider uses the OpenTelemetry SDK default sampler
(`ParentBased(AlwaysOn)`).

The `SYNTHIA_OTEL_SAMPLER` environment variable overrides the sampler at
runtime with one of:

- `always_on` (default)
- `always_off`
- `trace_id_ratio:0.1`

The parsed sampler is wrapped in `Sampler::ParentBased` so a parent
trace's sampling decision is honored.

## Trace Context Propagation

The W3C TraceContext propagator is registered on every
[`init_tracing`] call so `traceparent` / `tracestate` headers are
honored out of the box by [`extract_trace_context`] /
[`inject_trace_context`]. HTTP middleware in `synthia-server` calls
these helpers on every inbound request and outbound response so
upstream services see consistent trace correlation.