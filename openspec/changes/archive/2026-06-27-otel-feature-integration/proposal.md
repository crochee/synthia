## Why

`synthia-telemetry` 已存在 OTel 依赖（opentelemetry / opentelemetry-otlp / tracing-opentelemetry）和基础 `init_otlp_tracing` 函数，但存在 4 个生产级缺陷：(1) OTel 依赖是必选而非 feature-gated，违反 P3 按需加载原则并使 SDK 场景背负不必要的体积与编译开销；(2) 仅支持 OTLP gRPC exporter，缺乏 HTTP 协议支持，限制部署灵活性；(3) 缺失 codex 借鉴的 `SpanAttributesProcessor`，每个 span 需手写属性导致 DRY 违规与上下文丢失；(4) Agent runtime 关键路径（session/turn/llm.call/tool.execute/compaction/guardian.check）无自动 span 创建，违反 P9 可观测性原则。记忆中的多专家对抗性分析（2026-06-25）已将 OTel 集成升级为 P1-5，明确要求 cargo feature（默认禁用）+ SpanAttributesProcessor + OTLP gRPC/HTTP exporter，本次 change 落地该决议。

## What Changes

**OTel 依赖 feature-gated 化**
- From: `crates/synthia-telemetry/Cargo.toml` 中 `opentelemetry` / `opentelemetry-otlp` / `tracing-opentelemetry` / `opentelemetry_sdk` / `opentelemetry-semantic-conventions` 为必选依赖；`init_otlp_tracing` 无条件编译
- To: 新增 `otel` cargo feature（默认禁用），上述依赖移入 `[features] otel = [...]`；`init_otlp_tracing` 与 OTel 相关代码全部置于 `#[cfg(feature = "otel")]` 下；无 feature 时 `init_tracing` 退化为纯 console tracing
- Reason: P3 按需加载 + SDK 场景轻量化（个人 SDK / IDE 内嵌不需要 OTel 开销）
- Impact: 非破坏性（默认行为不变，启用 `otel` feature 时行为与当前一致）；CI 需增加 `--no-default-features` 与 `--features otel` 两条编译路径

**OTLP exporter 协议自动选择**
- From: `init_otlp_tracing` 仅构造 gRPC tonic exporter，endpoint 来自 `SYNTHIA_OTLP_ENDPOINT`
- To: 按 endpoint scheme 自动选择：`http://` → HTTP exporter（基于 `opentelemetry-otlp` 内置 reqwest），`grpc://` 或 `https://` → gRPC exporter；保留 `SYNTHIA_OTLP_ENDPOINT` 向后兼容（无 scheme 默认 gRPC）
- Reason: 部署灵活性（防火墙友好）+ 记忆明确要求双支持
- Impact: 非破坏性（现有 gRPC 行为保留）

**SpanAttributesProcessor 自动注入 span 上下文**
- From: 无 span 属性自动注入机制，每个 `#[tracing::instrument]` 需手写 `fields(session_id, turn_id, ...)`
- To: 新增 `SpanAttributesProcessor`（实现 `SpanProcessor` trait），在 `on_start` 时从 `SystemContext`（P1-4 Source/Epoch）和当前 `AgentRunContext` 提取并注入标准属性：`session.id` / `turn.id` / `agent.id` / `user.id` / `gen_ai.system` / `gen_ai.request.model`
- Reason: DRY + 标准化（codex `codex-rs/otel/` 借鉴）
- Impact: 非破坏性（新 span processor 装饰现有 tracer provider）

**Agent runtime 关键路径 span 集成**
- From: `Agent::run_stream` 关键路径无结构化 span；仅有零散 `tracing::info!` 日志
- To: 在 6 个关键边界创建 span：`session.start`/`session.end`（root）、`turn.start`/`turn.end`、`llm.call`、`tool.execute`、`compaction`、`guardian.check`；span 创建逻辑全部在 `#[cfg(feature = "otel")]` 下，无 feature 时退化为 no-op
- Reason: P9 可观测性 — 无法测量的无法优化
- Impact: 非破坏性（纯埋点，无行为变更）

## Capabilities

### New Capabilities

- `otel-feature-flag`: cargo feature `otel`（默认禁用）控制 OTel 依赖引入与编译路径；无 feature 时 `synthia-telemetry` 退化为纯 `tracing` console 输出
- `otlp-exporter-selection`: OTLP exporter 协议自动选择（gRPC / HTTP），按 endpoint scheme 路由，保留环境变量向后兼容
- `span-attributes-processor`: `SpanAttributesProcessor` 自动从 `SystemContext` 与 `AgentRunContext` 提取并注入标准 span 属性（session.id / turn.id / agent.id / user.id / gen_ai.*）
- `agent-runtime-spans`: Agent runtime 6 个关键路径边界的 span 创建（session / turn / llm.call / tool.execute / compaction / guardian.check），全部 feature-gated

### Modified Capabilities

无。现有 `observability` spec 关注文件级 Context Trace + Prometheus metrics + local alerts，与 OTel 集成是互补关系（不重叠）。`token-budget-observability` 关注 token 累计与告警，同样不重叠。

## Impact

**受影响代码**：
- `crates/synthia-telemetry/Cargo.toml` — 新增 `otel` feature，移动 OTel 依赖
- `crates/synthia-telemetry/src/tracer.rs` — `init_otlp_tracing` 重构为协议感知
- `crates/synthia-telemetry/src/span/attributes_processor.rs`（新）— `SpanAttributesProcessor` 实现
- `crates/synthia-telemetry/src/lib.rs` — `init_tracing` feature-gated 分支
- `crates/synthia-agent/src/agent.rs` — `Agent::run_stream` / `Agent::resume` 添加 span 创建
- `crates/synthia-agent/src/loop_context.rs` — turn span 边界
- `crates/synthia-agent/src/tools/` — `tool.execute` span
- `crates/synthia-llm/` — `llm.call` span
- `crates/synthia-guardian/src/review/reviewer.rs` — `guardian.check` span
- `crates/synthia-context/src/compaction/` — `compaction` span

**依赖**：无新外部依赖（`opentelemetry-otlp` 已含 HTTP exporter）；workspace `Cargo.toml` 可能需调整 opentelemetry 系列依赖的 feature 配置

**测试**：每个新 capability 需单元测试；feature on/off 编译矩阵需 CI 验证；SpanAttributesProcessor 需集成测试验证属性注入

**配置**：新增 `SYNTHIA_OTEL_SAMPLER` 环境变量（可选，默认 `ParentBased(AlwaysOn)`）
