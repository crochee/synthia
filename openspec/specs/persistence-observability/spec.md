# persistence-observability Specification

## Purpose
TBD - created by archiving change persistence-observability-gap-closure. Update Purpose after archive.
## Requirements
### Requirement: Event Store Seq Allocation Performance

The system SHALL allocate event sequence numbers in O(1) time using an in-process atomic counter, NOT by scanning the entire `events.jsonl` file on each append.

#### Scenario: First append initializes cache
- **WHEN** the system appends the first event to a session's `events.jsonl`
- **THEN** the system SHALL scan the file once to find `max_seq` and cache it in an atomic counter
- **AND** the new seq SHALL be `max_seq + 1`

#### Scenario: Subsequent appends use cache
- **WHEN** the system appends a subsequent event to the same session
- **THEN** the system SHALL allocate the seq via `fetch_add(1)` on the cached counter
- **AND** the system SHALL NOT scan the file

#### Scenario: Concurrent appends are atomic
- **WHEN** two threads concurrently append events to the same session
- **THEN** each append SHALL receive a unique, monotonically increasing seq
- **AND** no seq shall be duplicated

---

### Requirement: Event Store Crash Recovery

The system SHALL re-scan `events.jsonl` to find the true `max_seq` when the in-process cache is lost (e.g. after a crash), before allocating a new seq.

#### Scenario: Process restart after crash
- **WHEN** the process restarts and appends an event to an existing session
- **THEN** the system SHALL re-scan `events.jsonl` to find `max_seq`
- **AND** the new seq SHALL be `max_seq + 1`
- **AND** the system SHALL NOT reuse a seq that was already persisted

---

### Requirement: LatencyStats Accumulation Correctness

The system SHALL record latency values into the shared `LatencyStats` instance, not a clone, so that accumulated statistics (count, min, max, sum) are correct.

#### Scenario: Multiple latency recordings accumulate
- **WHEN** `record_llm_call` is called 3 times with latencies 100ms, 200ms, 300ms
- **THEN** `LatencyStats` SHALL report count=3, min=100, max=300, sum=600

#### Scenario: Quality score reflects latency
- **WHEN** `record_llm_call` has been called at least once
- **THEN** `compute_quality_score()` SHALL return a non-zero latency component

---

### Requirement: SessionInputQueue Durability

The system SHALL fsync the `session_input.jsonl` file after writing steering inputs, so that unconsumed inputs survive process crashes.

#### Scenario: Push fsyncs the file
- **WHEN** a steering input is pushed to `SessionInputQueue`
- **THEN** the file SHALL be fsynced (`sync_all`) before the call returns

#### Scenario: DrainPending fsyncs the rewritten file
- **WHEN** `drain_pending` rewrites the file with consumed markers
- **THEN** the rewritten file SHALL be fsynced before the call returns

---

### Requirement: Pruning Observability

The system SHALL emit tracing logs and OTel metrics when pruning marks messages, so that pruning behavior is observable.

#### Scenario: Prune emits tracing log
- **WHEN** `prune()` is called and marks at least one message
- **THEN** the system SHALL emit a `tracing::info!` log with `marked_count`, `kept_tokens`, `scanned_count`

#### Scenario: Prune creates span
- **WHEN** `prune()` is called
- **THEN** the system SHALL create an `info_span!("prune")` around the operation

#### Scenario: Prune emits OTel metrics (otel feature)
- **WHEN** `prune()` is called with the `otel` feature enabled
- **THEN** the system SHALL increment OTel counters `synthia.pruning.marked_count` and `synthia.pruning.kept_tokens`

---

### Requirement: OTel Sampler Configuration

The system SHALL read the `SYNTHIA_OTEL_SAMPLER` environment variable and configure the tracer provider's sampler accordingly.

#### Scenario: AlwaysOn sampler
- **WHEN** `SYNTHIA_OTEL_SAMPLER=always_on`
- **THEN** the tracer provider SHALL use `Sampler::ParentBased(AlwaysOn)`

#### Scenario: AlwaysOff sampler
- **WHEN** `SYNTHIA_OTEL_SAMPLER=always_off`
- **THEN** the tracer provider SHALL use `Sampler::ParentBased(AlwaysOff)`

#### Scenario: TraceIdRatio sampler
- **WHEN** `SYNTHIA_OTEL_SAMPLER=trace_id_ratio:0.1`
- **THEN** the tracer provider SHALL use `Sampler::ParentBased(TraceIdRatioBased(0.1))`

#### Scenario: Default sampler (env var unset)
- **WHEN** `SYNTHIA_OTEL_SAMPLER` is not set
- **THEN** the tracer provider SHALL default to `Sampler::ParentBased(AlwaysOn)` (current behavior)

---

### Requirement: Local Log File Persistence

The system SHALL persist tracing logs to a local file, so that logs survive process exit and are available for debugging.

#### Scenario: File logger writes to log dir
- **WHEN** tracing is initialized with a log directory
- **THEN** the system SHALL create a file logging layer writing to `{log_dir}/synthia.log` in append mode

#### Scenario: File logger excludes ANSI codes
- **WHEN** the file logger writes a log line
- **THEN** the line SHALL NOT contain ANSI color codes

#### Scenario: File logger includes standard fields
- **WHEN** the file logger writes a log line
- **THEN** the line SHALL include timestamp, level, target, and message fields

---

### Requirement: Cache Token Metrics Export

The system SHALL export OTel counters for LLM cache token usage, so that KV cache hit rate is observable.

#### Scenario: Cache read tokens exported (otel feature)
- **WHEN** an LLM provider response contains `cache_read_tokens`
- **AND** the `otel` feature is enabled
- **THEN** the system SHALL increment the OTel counter `synthia.llm.cache_read_tokens`

#### Scenario: Cache write tokens exported (otel feature)
- **WHEN** an LLM provider response contains `cache_write_tokens`
- **AND** the `otel` feature is enabled
- **THEN** the system SHALL increment the OTel counter `synthia.llm.cache_write_tokens`

#### Scenario: Input tokens exported for ratio computation
- **WHEN** an LLM provider response contains `input_tokens`
- **AND** the `otel` feature is enabled
- **THEN** the system SHALL increment the OTel counter `synthia.llm.input_tokens`
- **AND** the cache hit ratio SHALL be computable as `cache_read_tokens / input_tokens`

---

### Requirement: Metrics Exporter Protocol Detection

The metrics exporter SHALL detect the OTLP protocol from the endpoint URL scheme, consistent with the tracer's protocol detection logic.

#### Scenario: HTTP endpoint uses HTTP exporter
- **WHEN** `SYNTHIA_OTLP_ENDPOINT=http://localhost:4318`
- **THEN** the metrics exporter SHALL use OTLP HTTP protocol

#### Scenario: gRPC endpoint uses gRPC exporter
- **WHEN** `SYNTHIA_OTLP_ENDPOINT=grpc://localhost:4317`
- **THEN** the metrics exporter SHALL use OTLP gRPC protocol

#### Scenario: Protocol detection is shared with tracer
- **WHEN** the metrics exporter detects the protocol
- **THEN** it SHALL use the same `detect_protocol` function as the tracer, ensuring consistency

