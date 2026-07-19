## Context

`synthia-telemetry` crate 已存在并包含 OTel 相关代码骨架：`opentelemetry` 0.27 / `opentelemetry-otlp` / `tracing-opentelemetry` 依赖、`init_otlp_tracing` 函数（仅 gRPC）、`span/` 模块（含 `SpanBuilder` / `SpanKind` / 8 个 `create_*_span` 辅助函数）、`metrics/` 模块、`context_trace.rs`（文件级 trace）。

**关键约束**（来自记忆中的多专家对抗性分析 2026-06-25 + project_memory）：
- **P3 按需加载**：不预装可推迟的信息；OTel 在 SDK 场景（个人 SDK / IDE 内嵌）不需要
- **P9 可观测性**：每个关键路径必须有指标；无法测量的无法优化
- **P1 前缀一致性**：span 创建不能修改 prompt 前缀（span 是旁路观测，不进入上下文）
- **cargo feature 设计共识**：用 cargo feature flag 平衡企业级（重）与 SDK（轻）
- **明确不做**：Statsig exporter（codex 内部用，synthia 用 OTLP 即可）

**已完成的前置依赖**：
- P1-4 SystemContext Source/Epoch（2026-06-27 归档）— 提供 `user.id` / `session.id` / `agent.id` 上下文来源
- P1-1 AgentEvent ephemeral classification（2026-06-26 归档）— 提供 turn.id 稳定标识

**借鉴来源**：codex `codex-rs/otel/` — `SpanAttributesProcessor` + 多 exporter，不学 Statsig

**利益相关方**：
- 企业级 SaaS 部署：需要 OTLP 导出到 collector（Jaeger / Tempo / Honeycomb）
- 个人 SDK 用户：需要轻量编译，不引入 OTel 依赖
- IDE 内嵌 Agent：同 SDK，且不需要外部 collector
- opencode/codex Rust 替代场景：需要与企业级 OTel 栈互通

## Goals / Non-Goals

**Goals:**
- 将 OTel 依赖（opentelemetry / opentelemetry-otlp / tracing-opentelemetry / opentelemetry_sdk / opentelemetry-semantic-conventions）置于 `otel` cargo feature 下，默认禁用
- `init_tracing` 在无 `otel` feature 时退化为纯 `tracing-subscriber` console 输出，零 OTel 依赖
- OTLP exporter 按 endpoint scheme 自动选择 gRPC（tonic）或 HTTP（reqwest）
- 实现 `SpanAttributesProcessor`，在 span `on_start` 时自动注入 `session.id` / `turn.id` / `agent.id` / `user.id` / `gen_ai.system` / `gen_ai.request.model`
- Agent runtime 6 个关键路径边界自动创建 span，全部 feature-gated
- 向后兼容：现有 `SYNTHIA_OTLP_ENDPOINT` 行为保留；启用 `otel` feature 时与当前行为一致

**Non-Goals:**
- 不引入 OTel metrics exporter（metrics 保持现有 `MetricsCollector` 骨架，后续 P2 change）
- 不引入 Statsig exporter（记忆明确不做）
- 不引入 OTel logs API（用现有 `tracing` 即可）
- 不重构现有 `observability` spec（文件级 Context Trace + Prometheus + alerts 保持不变）
- 不修改 `token-budget-observability` spec
- 不引入新的 span 采样后端（仅用 OTel SDK 内置 sampler）
- 不实现分布式追踪跨进程传播（同进程内 span 上下文即可）

## Decisions

### D1：OTel 依赖置于 `otel` cargo feature 下，默认禁用

- **选择**：`synthia-telemetry` 的 `Cargo.toml` 新增 `[features] otel = ["dep:opentelemetry", "dep:opentelemetry-otlp", "dep:tracing-opentelemetry", "dep:opentelemetry_sdk", "dep:opentelemetry-semantic-conventions"]`，所有 OTel 依赖加 `optional = true`；OTel 相关代码全部置于 `#[cfg(feature = "otel")]` 下
- **理由**：P3 按需加载 + SDK 轻量化；记忆明确"必须是 cargo feature（默认禁用）"；codex `codex-rs/otel` 同样是 feature-gated
- **已考虑 alternative**：
  - (a) 新建 `synthia-otel` 独立 crate — 拒绝，因为 `synthia-telemetry` 已存在且包含相关代码，新建是重复
  - (b) 必选依赖 — 拒绝，违反 P3 且 SDK 场景背负不必要体积

### D2：在 `synthia-telemetry` 内 feature 分割，不新建 crate

- **选择**：保留单一 `synthia-telemetry` crate，OTel 代码用 `#[cfg(feature = "otel")]` 守卫
- **理由**：避免 workspace 复杂度；`tracing` / `tracing-subscriber` 是必选（轻量），OTel pipeline 才需要 feature
- **已考虑 alternative**：
  - (a) 新建 `synthia-otel` crate — 拒绝，重复
  - (b) 在 `synthia-core` 加 feature — 拒绝，违反 crate 职责分离

### D3：OTLP exporter 协议按 endpoint scheme 自动选择

- **选择**：`init_otlp_tracing` 解析 endpoint scheme：`http://` → HTTP exporter（`SpanExporter::builder().with_http()`），`grpc://` 或 `https://` 或无 scheme → gRPC exporter（`with_tonic()`，向后兼容）
- **理由**：部署灵活性（HTTP 防火墙友好）+ 记忆明确"OTLP gRPC/HTTP exporter"
- **已考虑 alternative**：
  - (a) 仅 gRPC — 拒绝，限制部署场景
  - (b) 仅 HTTP — 拒绝，破坏向后兼容
  - (c) 新增 `SYNTHIA_OTLP_PROTOCOL` 环境变量 — 拒绝，scheme 自动检测更直观

### D4：实现 `SpanAttributesProcessor`（codex 借鉴）

- **选择**：新增 `crates/synthia-telemetry/src/span/attributes_processor.rs`，实现 `opentelemetry_sdk::trace::SpanProcessor` trait，在 `on_start` 时通过 `tracing::Span::current()` 的 `Context` 提取 `SystemContext`（P1-4）和 `AgentRunContext`，注入 6 个标准属性：`session.id` / `turn.id` / `agent.id` / `user.id` / `gen_ai.system` / `gen_ai.request.model`
- **理由**：DRY（无需每个 `#[instrument]` 手写属性）+ 标准化（codex 模式）
- **已考虑 alternative**：
  - (a) 每个 `#[instrument]` 手写 `fields(session_id, ...)` — 拒绝，重复且易遗漏
  - (b) 用 OTel Baggage 传递 — 拒绝，Baggage 是 thread-local，跨 async 边界不可靠
  - (c) 用 `tracing::field::display` + 自定义 layer — 拒绝，与 SpanProcessor 重复

**上下文提取路径**：
- `SystemContext`（P1-4）通过 `tracing::Span::current().field("system_context")` 或 `tokio::task::yield_now()` 前注入的 `tracing::Span` 扩展获取
- 若上下文缺失（如 standalone 测试），processor 跳过属性注入，不报错（graceful degradation）

### D5：Agent runtime 6 个关键路径 span 边界

- **选择**：在以下边界创建 span，全部 `#[cfg(feature = "otel")]`：
  1. `session.start` / `session.end` — `Agent::run_stream` 入口/出口，root span
  2. `turn.start` / `turn.end` — `loop_context` 每次 turn 迭代
  3. `llm.call` — `synthia-llm` provider 调用前后
  4. `tool.execute` — `synthia-tool` registry 执行前后
  5. `compaction` — `synthia-context` compaction 触发
  6. `guardian.check` — `synthia-guardian` reviewer 调用
- **理由**：P9 可观测性 — 6 个边界覆盖所有关键性能/正确性观测点
- **已考虑 alternative**：
  - (a) 全量埋点（每个函数） — 拒绝，噪声大 + 范围蔓延
  - (b) 仅 session + turn — 拒绝，无法定位 LLM/工具/压缩瓶颈
  - (c) 用 `#[tracing::instrument]` 自动跨度 — 部分采用，但关键边界用手动 `tracing::span!` 控制 attribute

### D6：不引入 Statsig exporter

- **选择**：仅支持 OTLP exporter，不实现 Statsig
- **理由**：记忆"明确不做清单"：`✗ Statsig exporter（codex 内部用，synthia 用 OTLP 即可）`
- **已考虑 alternative**：
  - (a) 借鉴 codex 实现 Statsig — 拒绝，与记忆冲突

### D7：metrics 不纳入本次范围

- **选择**：本次仅做 tracing；metrics 保持现有 `MetricsCollector` 骨架，不引入 OTel metrics exporter
- **理由**：范围控制（~400 行预算）+ OTel metrics API 变动频繁
- **已考虑 alternative**：
  - (a) 含 OTel metrics — 拒绝，范围蔓延，后续 P2 单独 change

### D8：span 采样默认 `ParentBased(AlwaysOn)`

- **选择**：`SdkTracerProvider::builder().with_sampler(Sampler::ParentBased(Box::new(Sampler::AlwaysOn)))`；可通过 `SYNTHIA_OTEL_SAMPLER` 环境变量覆盖（`always_on` / `always_off` / `trace_id_ratio`）
- **理由**：跟随父 span 是 OTel 最佳实践；环境变量覆盖满足调试需求
- **已考虑 alternative**：
  - (a) 固定 `AlwaysOn` — 拒绝，无法调试时降采样
  - (b) 默认 `trace_id_ratio(0.1)` — 拒绝，开发场景丢失太多 span

### D9：HTTP exporter 用 `opentelemetry-otlp` 内置 reqwest

- **选择**：HTTP exporter 用 `SpanExporter::builder().with_http().with_endpoint(...)`，基于 `opentelemetry-otlp` 已含的 reqwest 特性
- **理由**：避免引入新依赖；与 gRPC exporter 同源（同一 `opentelemetry-otlp` crate）
- **已考虑 alternative**：
  - (a) 用 hyper 直接实现 — 拒绝，重复造轮子
  - (b) 用 reqwest crate 但独立于 `opentelemetry-otlp` — 拒绝，破坏一致性

## Risks / Trade-offs

- [Risk] OTel API/SDK 版本变动 — opentelemetry 0.27 已锁定，未来升级需同步 `tracing-opentelemetry` 与 `opentelemetry-otlp` → Mitigation: 在 workspace `Cargo.toml` 用 `version = "0.27"` 锁定；feature flag 隔离影响范围
- [Risk] `SpanAttributesProcessor` 跨 async 边界上下文丢失 — `tracing::Span::current()` 在 `.await` 点可能切换 → Mitigation: 用 `Instrument` 扩展显式传递 span；processor 检测到上下文缺失时 graceful skip（不报错）
- [Risk] feature flag 编译矩阵爆炸 — `otel` on/off × 各 crate 需测试 → Mitigation: CI 增加 `cargo check --no-default-features -p synthia-telemetry` 与 `cargo check --features otel -p synthia-telemetry` 两条路径；其他 crate 不受影响（不直接依赖 OTel）
- [Risk] 向后兼容破坏 — 现有 `init_otlp_tracing` 行为变更 → Mitigation: `otel` feature 启用时行为与当前完全一致；禁用时返回 `TracerInitResult::Console`
- [Trade-off] 仅 tracing 不含 metrics → 接受理由：范围控制 + OTel metrics API 不稳定 + 现有 Prometheus metrics 已覆盖核心指标
- [Trade-off] 6 个 span 边界而非全量埋点 → 接受理由：噪声控制 + 6 个边界已覆盖所有关键性能观测点
- [Trade-off] scheme 自动检测而非显式协议配置 → 接受理由：直观 + 向后兼容（无 scheme 默认 gRPC）

## Migration Plan

**部署顺序**：
1. 修改 `crates/synthia-telemetry/Cargo.toml`：新增 `otel` feature，OTel 依赖加 `optional = true`
2. 重构 `tracer.rs`：OTel 代码 `#[cfg(feature = "otel")]` 守卫；`init_tracing` 无 feature 时走 console 分支
3. 实现 `SpanAttributesProcessor`（`span/attributes_processor.rs`）
4. 重构 `init_otlp_tracing`：scheme 检测 + HTTP/gRPC 分支
5. Agent runtime 6 个边界 span 集成（feature-gated）
6. CI 增加 feature on/off 编译验证
7. 文档：`SYNTHIA_OTLP_ENDPOINT` scheme 说明 + `SYNTHIA_OTEL_SAMPLER` 说明

**Rollback 策略**：
- 若 feature-gated 引入编译问题：临时在 workspace `Cargo.toml` 的 `synthia-telemetry` 依赖加 `features = ["otel"]`，恢复必选行为
- 若 SpanAttributesProcessor 引入性能问题：在 `init_otlp_tracing` 中跳过 `with_span_processor` 装配
- 若 Agent runtime span 引入死锁/性能问题：删除对应 `#[cfg(feature = "otel")]` 块，无影响其他路径

**验收条件**：
- `cargo check -p synthia-telemetry --no-default-features` 通过（零 OTel 依赖）
- `cargo check -p synthia-telemetry --features otel` 通过
- `cargo test -p synthia-telemetry --features otel` 全部通过
- `cargo test --workspace` 全部通过（默认 feature 不破坏其他 crate）
- 启用 `otel` feature + `SYNTHIA_OTLP_ENDPOINT=http://localhost:4318` 时，span 导出到本地 OTLP collector
- 启用 `otel` feature + `SYNTHIA_OTLP_ENDPOINT=grpc://localhost:4317` 时，行为与当前一致
- SpanAttributesProcessor 在集成测试中验证 `session.id` 等属性已注入

## Open Questions

- **Q1**：`SpanAttributesProcessor` 如何从 `tracing::Span::current()` 提取 `SystemContext`？需确认 `SystemContext`（P1-4）是否已通过 `tracing::field` 或 span 扩展暴露。若未暴露，需在 P1-4 的 `SystemContext` 添加 `tracing::Value` 实现，或通过 `tokio::task_local` 传递。**决议**：实现时优先尝试 `tracing::field`，若不可行用 `task_local`。
- **Q2**：`SYNTHIA_OTEL_SAMPLER` 的 `trace_id_ratio` 参数如何指定比例？**决议**：`trace_id_ratio:0.1` 格式，解析失败时 fallback 到 `AlwaysOn`。
- **Q3**：Agent runtime span 是否需要记录 exception 事件？**决议**：是，在 `llm.call` / `tool.execute` 失败时记录 `exception` 事件（OTel convention），便于错误追踪。
