# Tasks: persistence-observability-gap-closure

## P0-A: events.jsonl seq 分配 O(n) → O(1)

- [x] 1.1 Add `last_seq_cache: Arc<DashMap<PathBuf, AtomicU64>>` field to `EventStore` (or a standalone `SeqCache` struct)
- [x] 1.2 Implement `get_or_init_seq(session_path) -> u64` that checks cache first, falls back to `max_seq` scan, stores in cache
- [x] 1.3 Modify `EventStore::append` to use `get_or_init_seq` + `fetch_add(1)` instead of `max_seq(session_path)? + 1`
- [x] 1.4 Unit test: first append scans file; second append uses cache (verify no second scan)
- [x] 1.5 Unit test: concurrent appends from 2 threads produce monotonically increasing, non-duplicate seqs
- [x] 1.6 Unit test: crash recovery — simulate cache loss, verify next append re-scans correctly

## P0-B: LatencyStats 不累积 bug 修复

- [x] 2.1 Change `EnhancedMetricsCollector.llm_latencies` from `Arc<LatencyStats>` to `Mutex<LatencyStats>`
- [x] 2.2 Modify `record_llm_call` to lock mutex and call `latencies.record(latency_ms)` directly
- [x] 2.3 Modify `record_llm_call_with_cache` to use the same pattern
- [x] 2.4 Fix any other callers that clone `llm_latencies`
- [x] 2.5 Unit test: call `record_llm_call` 3 times with different latencies, verify `LatencyStats` has count=3, correct min/max/sum
- [x] 2.6 Unit test: `compute_quality_score` returns non-zero latency component after `record_llm_call`

## P0-C: SessionInputQueue fsync

- [x] 3.1 Add `file.sync_all()` after `writeln!` in `SessionInputQueue::push`
- [x] 3.2 Add `file.sync_all()` after the rewrite loop in `SessionInputQueue::drain_pending`
- [x] 3.3 Unit test: verify `push` calls `sync_all` (use a spy/mock or check file durability on simulated crash)
- [x] 3.4 Unit test: verify `drain_pending` calls `sync_all`

## P0-D: pruning 可观测性

- [x] 4.1 Add `tracing::info_span!("prune", protect_tokens)` at the start of `prune()`
- [x] 4.2 Add `tracing::info!(target: "synthia.pruning", marked_count, kept_tokens, scanned_count, "prune completed")` before returning `PruneStats`
- [x] 4.3 Add `#[cfg(feature = "otel")]` OTel counter increment for `synthia.pruning.marked_count` and `synthia.pruning.kept_tokens`
- [x] 4.4 Unit test: verify `tracing` output contains prune stats (use `tracing_test` crate or mock subscriber)
- [x] 4.5 Unit test (otel feature): verify OTel counters are incremented

## P1-C: OTel sampler 接线

- [x] 5.1 Implement `parse_sampler(env: &str) -> Sampler` supporting `always_on` / `always_off` / `trace_id_ratio:<ratio>`
- [x] 5.2 Wrap with `Sampler::ParentBased(Box::new(inner))` for backward compatibility
- [x] 5.3 Read `SYNTHIA_OTEL_SAMPLER` env var in `init_otlp_tracing`, pass to `SdkTracerProvider::builder().with_sampler(sampler)`
- [x] 5.4 Unit test: `always_on` → `Sampler::AlwaysOn`; `always_off` → `Sampler::AlwaysOff`; `trace_id_ratio:0.1` → ratio 0.1
- [x] 5.5 Unit test: unset env var → default `ParentBased(AlwaysOn)`
- [x] 5.6 Update AGENTS.md to remove "尚未接线" note

## P1-D: 本地 logs 持久化

- [x] 6.1 Implement `init_file_logging(log_dir: &Path) -> Result<()>` that creates a file writer layer for `{log_dir}/synthia.log`
- [x] 6.2 Configure file layer: append mode, no ANSI, with timestamp/level/target/message
- [x] 6.3 Integrate file layer into `init_tracing` alongside console/OTLP layers
- [x] 6.4 Determine log_dir from env var `SYNTHIA_LOG_DIR` or fallback to `~/.synthia/logs/`
- [x] 6.5 Unit test: verify logs are written to file after `init_file_logging`
- [x] 6.6 Unit test: verify file is opened in append mode (existing content preserved)

## P1-E: cache 命中率指标导出

- [x] 7.1 Identify the provider response handling path where `usage` is extracted (likely in `synthia-provider`)
- [x] 7.2 Add a callback or channel to propagate `cache_read_tokens` / `cache_write_tokens` / `input_tokens` to `synthia-telemetry`
- [x] 7.3 Implement OTel counter export: `synthia.llm.cache_read_tokens`, `synthia.llm.cache_write_tokens`, `synthia.llm.input_tokens` (feature-gated)
- [x] 7.4 Wire the callback into the agent main loop's LLM response handling
- [x] 7.5 Unit test: mock provider response with cache tokens, verify counters incremented
- [x] 7.6 Unit test: provider response without cache tokens, verify counters not incremented (or zero)

## P1-F: metrics exporter HTTP 支持

- [x] 8.1 Refactor `metrics/otel.rs` to use `tracer::detect_protocol` for endpoint scheme detection
- [x] 8.2 Add OTLP HTTP exporter branch (using `opentelemetry_otlp::MetricExporter::builder().with_http()`)
- [x] 8.3 Unit test: `http://localhost:4318` → HTTP protocol; `grpc://localhost:4317` → gRPC protocol
- [x] 8.4 Integration test (otel feature): verify metrics export via HTTP endpoint (mock collector)

## Verification

- [x] 9.1 `cargo check --workspace` passes
- [x] 9.2 `cargo check --workspace --features otel` passes
- [x] 9.3 `cargo clippy --all-targets --all-features --tests --all` passes with 0 warnings
- [x] 9.4 `cargo +nightly fmt --all --check` passes
- [x] 9.5 `cargo test --workspace` passes
- [x] 9.6 `cargo test --workspace --features otel` passes
- [x] 9.7 `openspec validate persistence-observability-gap-closure` passes
