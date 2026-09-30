# synthia-telemetry

Observability primitives for the synthia agent framework: console/file
tracing, W3C trace-context propagation, Prometheus metrics, and
OpenTelemetry (OTel) OTLP export.

## Features — take only the half you use

The crate is three independent pieces, so a consumer that wants one of
them does not compile the others:

| Piece | Feature | Brings |
|---|---|---|
| console + optional file logging | *always compiled* | `tracing`, `tracing-subscriber` |
| Prometheus vectors + `/metrics` body | `metrics` | `prometheus`, `lazy_static` |
| OTLP trace export, W3C propagator | `otlp` | `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp` (gRPC via tonic **and** HTTP via reqwest), `tracing-opentelemetry` |

`default = ["metrics", "otlp"]` — the full stack, which is what a
deployment wants. `synthia-server` names both features explicitly.
`synthia-tool` takes `default-features = false, features = ["metrics"]`
because tool dispatch records the counters and has no spans of its own
to export; that keeps the exporter, its transports, and their TLS stack
out of every agent binary that does not run `synthia-server`.

```toml
# logs + tool metrics, no exporter
synthia-telemetry = { version = "0.1", default-features = false, features = ["metrics"] }
# logs only
synthia-telemetry = { version = "0.1", default-features = false }
```

`otlp` gates [propagation](#trace-context-propagation) too: W3C
`traceparent` extraction only means something when there is an SDK to
hand the span context to.

## OpenTelemetry (OTel)

Drop in the tracer via [`init_tracing`]; if `SYNTHIA_OTLP_ENDPOINT` is
unset, it falls back to console output.

```rust
use synthia_telemetry::{init_tracing, TelemetryConfig, TracingInit};

let config = TelemetryConfig::default();
match init_tracing(&config)? {
    // `Otlp` only exists with the `otlp` feature.
    TracingInit::Otlp(provider) => {
        // spans are exported to the OTLP collector; keep `provider`
        // alive for the duration of the process and call
        // `provider.shutdown()` on exit to flush pending spans
    }
    TracingInit::Console => {
        // tracing-subscriber fmt layer writes to stdout
    }
}
```

Nothing else needs to opt in — calling `init_tracing(&config)` starts
the OTLP pipeline when an endpoint is configured. The W3C TraceContext
propagator is registered automatically so `traceparent` /
`tracestate` headers are honored out of the box.

### OTLP Endpoint Configuration

Set `SYNTHIA_OTLP_ENDPOINT` to point at an OTLP collector. The transport
protocol (gRPC via tonic, or HTTP via reqwest) is auto-selected from the
URL scheme by `detect_protocol`:

| Scheme              | Port         | Protocol       | Example                               |
|---------------------|--------------|----------------|---------------------------------------|
| `grpc://`           | any          | gRPC (tonic)   | `grpc://localhost:4317`               |
| `https://`          | any          | gRPC (TLS)     | `https://collector.example.com:4317`  |
| `http://`           | `4317`       | gRPC           | `http://localhost:4317` (backward compat) |
| `http://`           | `4318`/other | HTTP (reqwest) | `http://localhost:4318`               |
| none / other        | any          | gRPC           | `localhost:4317`                      |

If `SYNTHIA_OTLP_ENDPOINT` is unset or empty, `init_tracing` falls back
to console tracing and returns `TracingInit::Console`.

### Sampler Configuration

The tracer provider uses the OpenTelemetry SDK default sampler
(`ParentBased(AlwaysOn)`).

`SYNTHIA_OTEL_SAMPLER` overrides the sampler at runtime with one of:

- `always_on` (default)
- `always_off`
- `trace_id_ratio:0.1`

The parsed sampler is wrapped in `Sampler::ParentBased` so a parent
trace's sampling decision is honored.

## Prometheus metrics

[`HTTP_REQUESTS_TOTAL`] and [`HTTP_REQUESTS_DURATION_SECONDS`] are the
RED vectors. They are labeled by `(method, matched_path)` where
`matched_path` is the axum route template, so a parameterized path like
`/api/v1/chat/sessions/{id}/messages` collapses to a single time series
regardless of how many distinct session ids are queried. The tool
vectors (`TOOL_EXECUTIONS_TOTAL`, `TOOL_EXECUTION_DURATION_SECONDS`,
`TOOL_PANICS_TOTAL`) are recorded by `synthia-tool`'s dispatch loop.

`synthia-server` mounts the per-request `track_metrics` middleware and
exposes a public `/metrics` endpoint. [`gather_text`] returns the
standard Prometheus text exposition body (version 0.0.4) for that
endpoint.

## Trace Context Propagation

The W3C TraceContext propagator is registered on every `init_tracing`
call so `traceparent` / `tracestate` headers are honored out of the box
by `extract_trace_context` / `inject_trace_context`. `synthia-server`'s
HTTP middleware calls these helpers on every inbound request and
outbound response so upstream services see consistent trace
correlation.
