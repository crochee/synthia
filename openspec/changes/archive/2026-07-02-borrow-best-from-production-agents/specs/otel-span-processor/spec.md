## ADDED Requirements

### Requirement: SpanAttributesProcessor SHALL inject 6 attributes on_start

The `SpanAttributesProcessor` MUST inject the following 6 span attributes on `on_start` (not `on_end`): `session.id`, `user.id`, `agent.id`, `turn.id`, `gen_ai.system`, `gen_ai.request.model`. The attributes MUST be available to exporters immediately after span creation, before any span events are recorded.

#### Scenario: All 6 attributes set on span start

- **WHEN** a new span is created with the `SpanAttributesProcessor` active
- **THEN** `on_start` is invoked before any span event
- **AND** the span has all 6 attributes set: `session.id`, `user.id`, `agent.id`, `turn.id`, `gen_ai.system`, `gen_ai.request.model`
- **AND** exporters can read these attributes immediately

#### Scenario: Missing context field uses empty string

- **WHEN** `on_start` is invoked but `user.id` is not available (e.g., anonymous session)
- **THEN** the `user.id` attribute is set to empty string `""`
- **AND** no panic occurs
- **AND** other attributes are still set normally

---

### Requirement: SpanAttributesProcessor SHALL NOT include Statsig exporter

The processor MUST NOT include any Statsig-related code or dependencies. The synthia telemetry stack uses OTLP gRPC/HTTP exporters only. Any reference to Statsig in the upstream codex implementation MUST be stripped during porting.

#### Scenario: Statsig code stripped

- **WHEN** the `SpanAttributesProcessor` is compiled
- **THEN** no `statsig` symbol appears in the binary
- **AND** no `statsig` crate is in the dependency tree

#### Scenario: OTLP exporter works without Statsig

- **WHEN** the `otel` feature is enabled with `SYNTHIA_OTLP_ENDPOINT` set
- **THEN** spans with `SpanAttributesProcessor`-injected attributes are exported via OTLP
- **AND** no Statsig-related errors or warnings appear in logs

---

### Requirement: SpanAttributesProcessor SHALL support gRPC and HTTP exporters

The processor MUST work with both OTLP gRPC exporter (default, port 4317) and OTLP HTTP exporter (when `SYNTHIA_OTLP_ENDPOINT` uses `http://` scheme). The processor itself is exporter-agnostic; the exporter selection is determined by `SYNTHIA_OTLP_ENDPOINT` scheme as documented in AGENTS.md.

#### Scenario: gRPC exporter (default)

- **WHEN** `SYNTHIA_OTLP_ENDPOINT = "grpc://collector:4317"` (or no scheme)
- **THEN** spans with processor-injected attributes are exported via gRPC
- **AND** the attributes are visible in the collector

#### Scenario: HTTP exporter

- **WHEN** `SYNTHIA_OTLP_ENDPOINT = "http://collector:4318"`
- **THEN** spans with processor-injected attributes are exported via HTTP
- **AND** the attributes are visible in the collector
