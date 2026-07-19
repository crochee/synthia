# Design: persistence-observability-gap-closure

## D1: P0-A events.jsonl seq 分配方案

### 问题
`EventStore::append` 调用 `max_seq(session_path)?`,后者 `fs::read_to_string` 全文件 + `rfind` 最后一行 + 反序列化。每次 append = O(n) 文件 I/O。

### 方案:进程内 AtomicU64 缓存

```rust
pub struct EventStore {
    // 进程内缓存,按 session_path 独立
    last_seq_cache: Arc<DashMap<PathBuf, AtomicU64>>,
}
```

**初始化策略**:
- 首次 append 某 session 时,调 `max_seq` 全文件扫描,写入 cache
- 后续 append 用 `cache.fetch_add(1, Relaxed)` 分配 seq
- `read_from` 不依赖 cache,仍从文件读(保证崩溃恢复正确性)

**并发安全**:
- `DashMap` 按 path 分桶,不同 session 不竞争
- 同 session 多线程 append:`fetch_add` 保证原子性
- fsync 后才返回,保证 seq 已持久化

**崩溃恢复**:
- 进程重启后 cache 丢失,首次 append 重新扫描 `max_seq`
- 如果文件最后一行损坏(写入未完成),`max_seq` 的 `serde_json::from_str` 失败 → 返回 error,调用方处理

### 为什么不用 SQLite?
- P10 原则:文件即记忆,Phase 0 不引入 SQLite
- opencode 用 SQLite 是因为 Effect-TS 生态有 Drizzle;synthia 的 JSONL 已够用,只需修 O(n) bug
- 如果未来需要复杂查询(跨会话分析),再考虑 SQLite 二级索引

### 为什么不用 mpsc 后台 task?
- codex 的 mpsc 方案需要后台 task + channel,增加复杂度
- synthia 的 `EventStore` 是同步 API,改 async 会波及调用链
- AtomicU64 缓存是最小侵入性方案

---

## D2: P0-B LatencyStats 修复方案

### 问题
```rust
// collector.rs:73-74 — clone 后修改不写回
let mut latencies = (*self.llm_latencies).clone();
latencies.record(latency_ms);
```

`Arc<LatencyStats>` clone 出一份,修改的是 clone,原 Arc 内的值不变。

### 方案:Mutex<LatencyStats>

```rust
pub struct EnhancedMetricsCollector {
    llm_latencies: Mutex<LatencyStats>,
    // ... 其他字段不变
}

pub fn record_llm_call(&self, latency_ms: u64, ...) {
    // ... atomic counters 不变
    let mut latencies = self.llm_latencies.lock().expect("poisoned");
    latencies.record(latency_ms);
}
```

### 为什么不用其他方案?
- `Arc<Mutex<LatencyStats>>`:多此一举,Mutex 本身可共享引用
- `parking_lot::Mutex`:可选,但 std::Mutex 够用(latency record 不是热路径)
- 保持 `Arc<LatencyStats>` + `compare_exchange` 更新:LatencyStats 内部有多个字段(min/max/count/sum),无法用单个原子操作更新

---

## D3: P0-D pruning 可观测性方案

### 问题
`prune()` 返回 `PruneStats` 但无任何观测输出。调用方(main_loop)也未记录。

### 方案:三层观测

```rust
pub fn prune(messages: &mut [Message], protect_tokens: u32) -> PruneStats {
    let _span = tracing::info_span!("prune", protect_tokens).entered();
    // ... 现有逻辑 ...

    tracing::info!(
        target: "synthia.pruning",
        marked_count = stats.marked_count,
        kept_tokens = stats.kept_tokens,
        scanned_count = stats.scanned_count,
        "prune completed"
    );

    // OTel metrics (feature-gated)
    #[cfg(feature = "otel")]
    {
        if let Some(meter) = opentelemetry::global::meter("synthia").u64_counter("synthia.pruning.marked_count").ok() {
            meter.add(stats.marked_count as u64, &[]);
        }
    }

    stats
}
```

### 为什么不把 metric 放调用方?
- `prune()` 是核心逻辑点,所有调用路径都会经过
- 调用方(main_loop)已经有自己的 span,再加 prune span 会重复
- `PruneStats` 仍返回,调用方可用于其他决策

---

## D4: P1-C OTel sampler 接线方案

### 问题
`SYNTHIA_OTEL_SAMPLER` 环境变量文档化但未实现。

### 方案

```rust
fn parse_sampler(env: &str) -> Sampler {
    match env {
        "always_on" => Sampler::AlwaysOn,
        "always_off" => Sampler::AlwaysOff,
        s if s.starts_with("trace_id_ratio:") => {
            let ratio: f64 = s["trace_id_ratio:".len()..].parse().unwrap_or(1.0);
            Sampler::TraceIdRatioBased(ratio)
        }
        _ => Sampler::AlwaysOn, // 默认
    }
}

// in init_otlp_tracing:
let sampler = std::env::var("SYNTHIA_OTEL_SAMPLER")
    .map(|s| parse_sampler(&s))
    .unwrap_or(Sampler::AlwaysOn);

SdkTracerProvider::builder()
    .with_resource(resource)
    .with_sampler(sampler)  // 新增
    .with_batch_exporter(exporter, Tokio)
    .with_span_processor(SpanAttributesProcessor::new())
    .build()
```

### 为什么用 ParentBased?
- AGENTS.md 文档说"当前使用 SDK 默认 `ParentBased(AlwaysOn)`"
- 保持向后兼容:未设置环境变量时行为不变
- 如果用户设 `trace_id_ratio:0.1`,实际效果是 `ParentBased(TraceIdRatioBased(0.1))`

**修正**:实际应包裹 ParentBased:

```rust
let inner_sampler = parse_sampler(&env_str);
let sampler = Sampler::ParentBased(Box::new(inner_sampler));
```

---

## D5: P1-D 本地 logs 持久化方案

### 问题
无本地日志文件,debug 只能靠 console 输出(进程退出即丢失)。

### 方案:文件日志 layer

```rust
pub fn init_file_logging(log_dir: &Path) -> Option<NonZeroU64> {
    let path = log_dir.join("synthia.log");
    let file = OpenOptions::new().create(true).append(true).open(&path).ok()?;
    let layer = tracing_subscriber::fmt::layer()
        .with_writer(file)
        .with_ansi(false) // 文件不要 ANSI 颜色
        .with_target(true)
        .with_level(true);
    // 注册到 global subscriber
    Some(...)
}
```

### 日志轮转
- Phase 0: 不做轮转,依赖外部工具(logrotate)
- Phase 2: 可选加 `tracing-appender` 的 `RollingFileAppender`(按日轮转)

### 为什么不做 SQLite logs?
- codex 的 LogDbLayer 需要 sqlx 依赖 + schema migration,Phase 0 过重
- P10 原则:文件即记忆
- 文件日志 + grep/ripgrep 足够 debug 场景
- 如果未来需要按 session_id 查询日志,再考虑 SQLite

---

## D6: P1-E cache 命中率指标导出方案

### 问题
- `CacheBreakDetector` dead code(无生产调用)
- `EnhancedMetricsCollector.total_cached_tokens` 累积但无 OTel 导出
- `AgentMetrics.cache_hits_total/misses_total` deprecated

### 方案:从 provider 响应提取 + OTel 导出

**数据来源**:provider 响应中的 `usage` 字段已包含 `cache_read_tokens` / `cache_write_tokens`(Anthropic API 原生字段)。

**导出路径**:
1. `synthia-provider` 的 response 处理路径提取 cache tokens
2. 通过回调或 channel 传给 `synthia-telemetry`
3. OTel counter 导出:

```rust
#[cfg(feature = "otel")]
{
    let meter = opentelemetry::global::meter("synthia");
    let cache_read = meter.u64_counter("synthia.llm.cache_read_tokens").ok();
    let cache_write = meter.u64_counter("synthia.llm.cache_write_tokens").ok();
    let input_tokens = meter.u64_counter("synthia.llm.input_tokens").ok();

    if let Some(c) = cache_read { c.add(usage.cache_read_tokens, &[]); }
    if let Some(c) = cache_write { c.add(usage.cache_write_tokens, &[]); }
    if let Some(c) = input_tokens { c.add(usage.input_tokens, &[]); }
}
```

**cache_hit_ratio**:不在运行时计算,留给 dashboard/查询层计算(`cache_read / input_tokens`)。OTel gauge 适合周期性快照,但 synthia 的 metrics 是 push 模式,counter 更自然。

### 为什么不修复 CacheBreakDetector?
- `CacheBreakDetector` 的设计是"检测 cache 是否 break"(prefix 变化),与"cache 命中率"是不同概念
- cache 命中率 = provider 返回的 `cache_read_tokens / input_tokens`
- CacheBreakDetector 的 dead code 问题应在单独的 change 中处理(清理或接线)

---

## D7: P1-F metrics exporter HTTP 支持

### 问题
`metrics/otel.rs` 硬编码 tonic gRPC,不像 `tracer.rs` 做 scheme 检测。

### 方案:复用 detect_protocol

```rust
// metrics/otel.rs
use crate::tracer::detect_protocol;

pub fn init_metrics(config: &TelemetryConfig) -> Option<MeterProvider> {
    let endpoint = std::env::var("SYNTHIA_OTLP_ENDPOINT").ok()?;
    let protocol = detect_protocol(&endpoint);

    let exporter = match protocol {
        OtlpProtocol::Grpc => {
            // 现有 gRPC 路径
            opentelemetry_otlp::MetricExporter::builder()
                .with_tonic()
                .with_endpoint(&endpoint)
                .build()?
        }
        OtlpProtocol::Http => {
            // 新增 HTTP 路径
            opentelemetry_otlp::MetricExporter::builder()
                .with_http()
                .with_endpoint(&endpoint)
                .build()?
        }
    };

    let reader = PeriodicReader::builder(exporter)
        .with_interval(Duration::from_secs(30))
        .build();

    let provider = SdkMeterProvider::builder()
        .with_reader(reader)
        .build();

    global::set_meter_provider(provider.clone());
    Some(provider)
}
```

---

## 风险与缓解

| 风险 | 概率 | 影响 | 缓解 |
|------|------|------|------|
| P0-A AtomicU64 缓存与文件不一致 | 低 | seq 重复 | 首次扫描 + fsync 保证;崩溃后重新扫描 |
| P0-B Mutex poisoning | 低 | metric 丢失 | `expect("poisoned")` 让 panic 显性化 |
| P0-C fsync 性能影响 | 低 | steering 延迟 +1-5ms | 可接受;steering 不是热路径 |
| P1-D 日志文件增长 | 中 | 磁盘满 | 文档建议配 logrotate;Phase 2 加 RollingFileAppender |
| P1-E provider response 字段缺失 | 低 | metric 为零 | 用 Option + unwrap_or(0) |

## 不做的事

- ❌ 引入 SQLite (P10 原则,Phase 0 不需要)
- ❌ 修复 CacheBreakDetector dead code (单独 change 处理)
- ❌ 统一两套 SpanContext (单独 change 处理)
- ❌ 统一两套 prefix hash (单独 change 处理)
- ❌ 后台反刍日志到 SQLite (Phase 2 考虑)
- ❌ trace bundle (codex 式推理证据,Phase 2 可选)
