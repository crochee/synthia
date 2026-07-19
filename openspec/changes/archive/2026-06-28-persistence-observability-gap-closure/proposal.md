# Proposal: persistence-observability-gap-closure

## Why

Synthia 的持久化与可观测性层与生产级 agent (opencode / codex-rs) 存在 4 个 bug 级差距和 4 个架构级差距。这些差距在多专家对抗性分析(持久化/可观测专家视角)中被识别,并已通过源码验证确认真实存在。

**已排除的假差距**(归档变更已解决):
- ~~tool-output-store~~ → `2026-06-23-tool-output-offloading` 已落地 (`synthia-context/src/truncate/`)
- ~~context-epoch 正向控制~~ → `2026-06-27-system-context-source-epoch` 已实现 (worktree,未提交)
- ~~applyCachePolicy 接线~~ → `2026-06-25-kv-cache-policy-injection` 已落地 (4 个 assembler 路径 + anthropic transform)
- ~~OTel span 体系~~ → `2026-06-27-otel-feature-integration` 已在 master (session/turn/llm/tool/compaction/guardian span)

**本次聚焦的真实差距**(全部经源码验证):

| # | 差距 | 严重度 | 验证证据 |
|---|------|--------|----------|
| P0-A | `events.jsonl` O(n) seq 分配 | bug | `store/events.rs:77` 每次 append 重读全文件 |
| P0-B | `LatencyStats` clone-then-record 不写回 | bug | `agent_metrics/collector.rs:73-74` clone 后修改丢弃 |
| P0-C | `SessionInputQueue` push/drain 未 fsync | bug | `store/session_input.rs:116-121` 无 sync_all |
| P0-D | `prune()` 完全无可观测性 | bug | `pruning/engine.rs:64-95` 无 tracing/metric |
| P1-C | OTel sampler 未接线 | arch | `tracer.rs` 无 SAMPLER 环境变量读取 |
| P1-D | 本地 logs 无持久化 | arch | 无文件日志,无 SQLite logs (对比 codex LogDbLayer) |
| P1-E | cache 命中率指标未导出 | arch | `CacheBreakDetector` 是 dead code;`cached_tokens` 累积但无 metric |
| P1-F | metrics exporter 仅 gRPC | arch | `metrics/otel.rs` 不像 tracer.rs 做 HTTP 检测 |

## What Changes

### P0-A: events.jsonl seq 分配 O(n) → O(1)

- **From**: `EventStore::append` 每次调用 `max_seq(session_path)?`,后者 `fs::read_to_string` 全文件 + `rfind` 最后一行 + 反序列化 ([events.rs:153-173](file:///home/crochee/workspace/synthia/crates/synthia-session/src/store/events.rs))
- **To**: 进程内缓存 `last_seq: AtomicU64`,首次访问时全文件扫描初始化,后续 append 用 `fetch_add(1)` 分配。`read_from` 仍从文件读以保证崩溃恢复后的正确性。
- **Reason**: 长会话(5000+ 事件)每次 append 扫描全文件,延迟线性增长。opencode 用 SQLite `event_sequence.latest+1` (O(1)),codex 用 mpsc 串行化 (O(1))。
- **Impact**: 非破坏性;`max_seq` 函数保留用于初始化路径。

### P0-B: LatencyStats 不累积 bug 修复

- **From**: `record_llm_call` 中 `let mut latencies = (*self.llm_latencies).clone(); latencies.record(latency_ms);` ([collector.rs:73-74](file:///home/crochee/workspace/synthia/crates/synthia-telemetry/src/agent_metrics/collector.rs)) — clone 后修改不写回 `Arc<LatencyStats>`,latency 统计永远为空
- **To**: 用 `Mutex<LatencyStats>` 替代 `Arc<LatencyStats>` + clone,直接在锁内 `record`
- **Reason**: latency 分布统计(min/max/avg)完全不工作,`compute_quality_score` 的 latency 权重永远为零
- **Impact**: 非破坏性;内部实现修复,API 不变

### P0-C: SessionInputQueue fsync

- **From**: `push` 仅 `writeln!` 无 `sync_all` ([session_input.rs](file:///home/crochee/workspace/synthia/crates/synthia-session/src/store/session_input.rs));`drain_pending` 用 `File::create` 重写整个文件也无 `sync_all`
- **To**: 两处加 `file.sync_all()`
- **Reason**: 崩溃时未消费的 steering 输入丢失。对比 codex rollout 每行 `flush().await` + oneshot ack。
- **Impact**: 非破坏性;仅增加 fsync 调用

### P0-D: pruning 可观测性

- **From**: `prune()` 返回 `PruneStats { marked_count, kept_tokens, scanned_count }` 但无 tracing 日志、无 OTel span、无 metric 上报 ([engine.rs:64-95](file:///home/crochee/workspace/synthia/crates/synthia-context/src/pruning/engine.rs))
- **To**: (1) 加 `tracing::info!` 日志记录 prune 事件;(2) 加 OTel metric `synthia.pruning.marked_count` / `synthia.pruning.kept_tokens`;(3) 在调用方(main_loop)加 `tracing::info_span!("prune")` span
- **Reason**: agent_rule.md P9 要求 `pruning_stage_distribution` 指标,当前完全无法观测 pruning 行为
- **Impact**: 非破坏性;纯增加观测点

### P1-C: OTel sampler 接线

- **From**: `SYNTHIA_OTEL_SAMPLER` 环境变量在 AGENTS.md 文档化为"设计已定但尚未接线",当前使用 SDK 默认 `ParentBased(AlwaysOn)` ([tracer.rs](file:///home/crochee/workspace/synthia/crates/synthia-telemetry/src/tracer.rs))
- **To**: 读取 `SYNTHIA_OTEL_SAMPLER` 环境变量,支持 `always_on` / `always_off` / `trace_id_ratio:0.1` 三种采样器,传入 `SdkTracerProvider::builder().with_sampler(sampler)`
- **Reason**: 生产环境需要可控采样率,全量采样在高负载下成本过高
- **Impact**: 非破坏性;未设置环境变量时行为不变(默认 AlwaysOn)

### P1-D: 本地 logs 持久化

- **From**: 无本地日志持久化,所有 `tracing` 日志仅输出到 console ([telemetry](file:///home/crochee/workspace/synthia/crates/synthia-telemetry/src))
- **To**: 引入文件日志 layer,写入 `{log_dir}/synthia.log`(append 模式),支持日志轮转。对比 opencode `logging.ts` 的 `fileLogger` + codex `LogDbLayer`(SQLite logs 表)。
- **Reason**: 生产环境需要本地日志用于 debug 和审计。codex 的 LogDbLayer 还支持按 thread_id 查询和自动 prune,但 Phase 0 先做文件日志(对齐 P10 文件即记忆原则)。
- **Impact**: 非破坏性;新增 layer,不影响现有 console/OTLP 输出

### P1-E: cache 命中率指标导出

- **From**: (1) `CacheBreakDetector::check_cache_break` 是 dead code,无生产调用者(仅 tests);(2) `EnhancedMetricsCollector` 累积 `total_cached_tokens` 但无 metric 导出;(3) `agent_metrics` 的 `AgentMetrics` 有 `cache_hits_total`/`cache_misses_total` 字段但标记为 deprecated
- **To**: (1) 从 provider 响应提取 `cache_read_tokens` / `cache_write_tokens`,通过 OTel counter 导出 `synthia.llm.cache_read_tokens` / `synthia.llm.cache_write_tokens`;(2) 计算 `cache_hit_ratio = cache_read_tokens / input_tokens` 并导出为 gauge
- **Reason**: agent_rule.md P9 要求 `cache_hit_ratio` 指标,当前无法观测 KV cache 命中率
- **Impact**: 非破坏性;新增 metric 导出点

### P1-F: metrics exporter HTTP 支持

- **From**: `metrics/otel.rs` 仅支持 tonic gRPC,不像 `tracer.rs` 那样做 HTTP scheme 检测 ([metrics/otel.rs](file:///home/crochee/workspace/synthia/crates/synthia-telemetry/src/metrics))
- **To**: 复用 `tracer.rs` 的 `detect_protocol` 逻辑,HTTP scheme 时用 OTLP HTTP exporter
- **Reason**: 与 tracer 管线不一致,HTTP endpoint 环境下 metrics 无法导出
- **Impact**: 非破坏性;未设置环境变量时行为不变

## Capabilities

### New Capabilities
- `persistence-observability`: 修复持久化层 bug + 补齐可观测性指标导出,使 synthia 的持久化和可观测性达到生产级标准。

## Impact

- **Affected crates**: `synthia-session`, `synthia-telemetry`, `synthia-context`, `synthia-agent`
- **Breaking changes**: 无
- **Performance impact**: P0-A 修复后长会话 append 延迟从 O(n) 降到 O(1);P0-C 增加 fsync 有轻微写入延迟
- **Test impact**: 每个 P0/P1 项需新增单元测试;P0-B 需回归测试验证 latency 累积
