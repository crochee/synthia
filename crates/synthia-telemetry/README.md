# synthia-telemetry

Observability primitives for the synthia agent framework: tracing setup,
context-trace recording, metrics, and optional OpenTelemetry (OTel) integration.

## OTel Integration

The `otel` cargo feature (default disabled) enables OpenTelemetry tracing with
OTLP export. When the feature is off, the crate compiles with **zero OTel
dependencies** and [`init_tracing`] falls back to console-only output.

### Enabling OTel

```toml
[dependencies]
synthia-telemetry = { version = "...", features = ["otel"] }
```

Downstream crates (e.g. `synthia-agent`) expose their own `otel` feature that
forwards to this one:

```toml
[dependencies]
synthia-agent = { version = "...", features = ["otel"] }
```

### OTLP Endpoint Configuration

Set the `SYNTHIA_OTLP_ENDPOINT` environment variable to point at an OTLP
collector. The transport protocol (gRPC via tonic, or HTTP via reqwest) is
auto-selected from the URL scheme by [`detect_protocol`]:

| Scheme              | Port         | Protocol     | Example                            |
|---------------------|--------------|--------------|------------------------------------|
| `grpc://`           | any          | gRPC (tonic) | `grpc://localhost:4317`            |
| `https://`          | any          | gRPC (TLS)   | `https://collector.example.com:4317` |
| `http://`           | `4317`       | gRPC         | `http://localhost:4317` (backward compat) |
| `http://`           | `4318`/other | HTTP (reqwest) | `http://localhost:4318`          |
| none / other        | any          | gRPC         | `localhost:4317`                   |

If `SYNTHIA_OTLP_ENDPOINT` is unset or empty, [`init_tracing`] falls back to
console tracing and returns [`TracerInitResult::Console`].

### Sampler Configuration

The tracer provider uses the OpenTelemetry SDK default sampler
(`ParentBased(AlwaysOn)`).

The `SYNTHIA_OTEL_SAMPLER` environment variable is specified by the design
(see `openspec/changes/otel-feature-integration/design.md`, decision D8) to
override the sampler at runtime with one of:

- `always_on` (default)
- `always_off`
- `trace_id_ratio:0.1`

> Note: the env-var override is not yet wired up in `init_otlp_tracing`; the
> provider currently relies on the SDK default. Sampler plumbing will land in a
> follow-up task.

### Span Attributes Processor

[`SpanAttributesProcessor`] is a `SpanProcessor` that auto-injects the
following attributes on every span's `on_start` (when the `otel` feature is
enabled). Values are read from `tokio::task_local!` scopes established by
`synthia-agent` around `Agent::run_stream`:

| Attribute              | Source task-local        | SemConv? |
|------------------------|---------------------------|----------|
| `session.id`           | `SESSION_ID`              | yes      |
| `user.id`              | `USER_ID`                 | yes      |
| `agent.id`             | `AGENT_ID`                | no (synthia-specific) |
| `turn.id`              | `TURN_ID`                 | no (synthia-specific) |
| `gen_ai.system`        | `GEN_AI_SYSTEM`           | yes      |
| `gen_ai.request.model` | `GEN_AI_REQUEST_MODEL`    | yes      |

When a task-local value is absent (e.g. in standalone tests), the processor
gracefully skips that attribute - no panic, no error log.

### Agent Runtime Spans

Six critical-path spans are created by the agent runtime, all
`#[cfg(feature = "otel")]`-gated:

| Span name         | Crate              | Boundary                                  |
|-------------------|--------------------|-------------------------------------------|
| `session.start`   | `synthia-agent`    | Root span for `Agent::run_stream`         |
| `turn.start`      | `synthia-agent`    | Per-turn iteration                        |
| `llm.call`        | `synthia-provider` | LLM provider call (Anthropic / OpenAI)    |
| `tool.execute`    | `synthia-tool`     | Tool execution in the registry            |
| `compaction`      | `synthia-context`  | Context compaction                        |
| `guardian.check`  | `synthia-guardian` | Guardian reviewer check                   |

Span creation is a **bystander observation**: it never modifies the
`CompletionRequest` payload (messages / system / tools), so the KV-cache prefix
and `prompt_cache_key` inputs remain byte-identical with the feature on or off
(verified by `crates/synthia-agent/tests/otel_prefix_stability.rs`).

### Quick Start

```rust
use synthia_telemetry::{init_tracing, TelemetryConfig, TracerInitResult};

// Enable OTLP export by setting SYNTHIA_OTLP_ENDPOINT before calling this.
// Without the env var (or without the `otel` feature), falls back to console.
let config = TelemetryConfig::default();
match init_tracing(&config)? {
    TracerInitResult::Otlp(provider) => {
        // spans are exported to the OTLP collector
        // keep `provider` alive for the duration of the process; call
        // `provider.shutdown()` on exit to flush pending spans
    }
    TracerInitResult::Console => {
        // tracing-subscriber fmt layer writes to stdout
    }
}
```
