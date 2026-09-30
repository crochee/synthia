<!--
Raw capture of brainstorming output.

本檔原樣捕捉 brainstorming skill 的產出，不強制結構。
Skill 的自然產出通常是 decision log 格式（背景 → 決議鏈 Q1-Qn → 設計取捨），
但依對話內容可能有不同組織方式。

design.md 從本檔萃取並重新整理為結構化設計文件。

不要將本檔的內容複製到 design.md — design.md 是獨立的重組產物，
兩者互補但不重疊。
-->

# Brainstorm: P1-5 OTel 集成（feature = "otel"）

## 背景

记忆中的多专家对抗性分析（2026-06-25）已为 P1-5 OTel 集成定下基调：

> **P1-5 OTel 集成（feature = "otel"，~400 行）**：从 P2 升级，SpanAttributesProcessor + OTLP gRPC/HTTP exporter

借鉴来源映射：
> P1-5 OTel → codex `codex-rs/otel/`（SpanAttributesProcessor + 多 exporter，不学 Statsig）

同时受 project memory 中的 P3 按需加载原则约束：
> 不预装任何可以推迟加载的信息。
> 判断标准：如果一段信息放进 system prompt 后，有 >30% 的 session 不会用到它，就不该预装。

当前 `synthia-telemetry` crate 状态（探索结论）：
- 已有 `opentelemetry` / `opentelemetry-otlp` / `tracing-opentelemetry` 依赖（**必选**）
- 已有 `init_otlp_tracing` 函数（仅 OTLP gRPC，无 HTTP）
- 已有 span / metrics / tracer 模块骨架
- 缺失：`otel` cargo feature flag、HTTP exporter、SpanAttributesProcessor、Agent runtime 集成

## 决议链

### Q1：opentelemetry 依赖应该是必选还是 feature-gated？

**对抗性审查结论**：必选依赖违反 P3 + P1 SDK 场景轻量化原则。

- 论据 A（必选）：synthia-telemetry 已经有 OTel 依赖，移除需要重构
- 论据 B（feature-gated）：记忆中明确"必须是 cargo feature（默认禁用，SDK 场景轻量）"
- 论据 C（feature-gated）：codex `codex-rs/otel` 是独立 crate + feature flag
- 论据 D（feature-gated）：SDK 场景（个人 SDK、IDE 内嵌）不需要 OTel 开销

**决议**：feature-gated。`synthia-telemetry` 的 OTel 依赖放到 `otel` feature 下，默认禁用。无 `otel` feature 时，`init_tracing` 退化为纯 console tracing，无 OTel 依赖引入。

### Q2：feature flag 边界划在哪里？

**对抗性审查结论**：在 `synthia-telemetry` 内部分割，而非新建 crate。

- 论据 A（新建 `synthia-otel` crate）：最干净的隔离，但增加 workspace 复杂度
- 论据 B（`synthia-telemetry` 内 feature 分割）：保留单一 crate，OTel 代码在 `#[cfg(feature = "otel")]` 下
- 论据 C（codex 模式）：codex 是独立 `codex-rs/otel/` crate + feature flag

**决议**：B（`synthia-telemetry` 内 feature 分割）。理由：
1. synthia-telemetry 已经存在且包含相关代码，新建 crate 是重复
2. 现有 `tracing` / `tracing-subscriber` 是必选（轻量），OTel 相关才需要 feature
3. SDK 场景只需 `tracing` console 输出，不需要 OTel pipeline

### Q3：SpanAttributesProcessor 是什么？为什么需要？

**对抗性审查结论**：codex 借鉴的核心组件，自动注入 span 属性。

codex 的 `SpanAttributesProcessor` 是一个 `SpanProcessor` 装饰器，在 span `on_start` 时自动注入标准属性：
- `session.id` — 当前 session ID
- `turn.id` — 当前 turn ID
- `agent.id` — agent 实例 ID
- `user.id` — 用户 ID（来自 SystemContext Source/Epoch，P1-4 已完成）
- `gen_ai.system` — "anthropic" / "openai"
- `gen_ai.request.model` — 模型名

这样所有 span 自动带上下文，无需在每个 `#[instrument]` 处重复写属性。

**决议**：实现 `SpanAttributesProcessor`，从 `SystemContext` (P1-4) 和 `AgentRunContext` 提取上下文，通过 `Baggage` 或 scoped context 注入。

### Q4：OTLP exporter 协议选择？

**对抗性审查结论**：gRPC + HTTP 双支持，运行时按 endpoint scheme 选择。

- 论据 A（仅 gRPC）：当前已有，性能好
- 论据 B（仅 HTTP）：防火墙友好，调试容易
- 论据 C（双支持）：记忆明确"OTLP gRPC/HTTP exporter"，按 endpoint 自动选择

**决议**：C（双支持）。
- `http://` endpoint → HTTP exporter
- `grpc://` 或 `https://` endpoint → gRPC exporter
- 默认 gRPC（向后兼容现有 `SYNTHIA_OTLP_ENDPOINT` 行为）

### Q5：Agent runtime 集成深度？

**对抗性审查结论**：在关键路径自动创建 span，但保持 opt-in。

关键 span 边界（按 P9 可观测性原则）：
1. `session.start` / `session.end` — Session 级 root span
2. `turn.start` / `turn.end` — 每个 turn 一个 span
3. `llm.call` — 每次 LLM 调用
4. `tool.execute` — 每次工具执行
5. `compaction` — 上下文压缩
6. `guardian.check` — Guardian 审查

**决议**：
- 在 `Agent::run_stream` 关键路径添加 `#[tracing::instrument]` + 手动 span 创建
- span 创建逻辑全部在 `#[cfg(feature = "otel")]` 下
- 无 `otel` feature 时，`#[instrument]` 退化为 no-op（tracing 默认行为）

### Q6：是否引入 Statsig exporter？

**对抗性审查结论**：明确不做。

记忆中"明确不做清单"：
> ✗ Statsig exporter（codex 内部用，synthia 用 OTLP 即可）

**决议**：不引入。仅支持 OTLP exporter。

### Q7：metrics 是否纳入本次范围？

**对抗性审查结论**：分阶段，本次仅做 tracing，metrics 已有骨架不破坏。

- 论据 A（含 metrics）：codex 同时做 tracing + metrics
- 论据 B（仅 tracing）：metrics 已有 `MetricsCollector` 骨架，且 OTel metrics API 变动频繁
- 论据 C（仅 tracing）：聚焦 ~400 行预算，避免范围蔓延

**决议**：B（仅 tracing）。metrics 保持现状，不在本次引入 OTel metrics exporter。后续可作为 P2 单独 change。

## 设计取舍总结

| 决策 | 选择 | 拒绝的替代 | 理由 |
|------|------|-----------|------|
| 依赖模型 | cargo feature `otel`（默认禁用） | 必选依赖 | P3 按需加载 + SDK 轻量化 |
| 代码组织 | `synthia-telemetry` 内 `#[cfg(feature = "otel")]` | 新建 `synthia-otel` crate | 避免重复，已有 crate 复用 |
| Span 属性注入 | `SpanAttributesProcessor`（codex 借鉴） | 每个 `#[instrument]` 手写 | DRY + 标准化 |
| OTLP 协议 | gRPC + HTTP 双支持 | 仅 gRPC / 仅 HTTP | 记忆明确 + 部署灵活性 |
| Agent 集成 | 关键路径 6 个 span 边界 | 全量埋点 / 不集成 | P9 可观测性 + 范围控制 |
| Statsig | 不做 | 借鉴 codex | 记忆明确不做 |
| Metrics | 不纳入本次 | 含 OTel metrics | 范围控制 + API 稳定性 |

## 风险与缓解

1. **OTel API/SDK 版本变动**：opentelemetry 0.27 已锁定，未来升级需同步 `tracing-opentelemetry`。缓解：在 `Cargo.toml` 用 `version = "0.27"` 锁定，feature flag 隔离影响。

2. **SpanAttributesProcessor 跨线程上下文**：`SystemContext` 是 `Arc` 共享，但 OTel Baggage 是 thread-local。缓解：用 `tracing::Span::current()` + `Instrument` 扩展传递，避免直接用 Baggage。

3. **feature flag 编译矩阵**：`otel` on/off × 各 crate 需测试。缓解：CI 增加 `cargo check --no-default-features` 和 `cargo check --features otel` 两条路径。

4. **向后兼容**：现有 `init_otlp_tracing` 行为必须保持。缓解：`otel` feature 启用时行为不变，禁用时该函数返回 `TracerInitResult::Console`。

## 未决问题（自行决议，不阻塞）

- **Q8：HTTP exporter 用 hyper 还是 reqwest？** 自行决议：用 `opentelemetry-otlp` 内置的 HTTP exporter（基于 `reqwest`），与 gRPC exporter 同源，避免引入新依赖。

- **Q9：span 采样率？** 自行决议：默认 `ParentBased(AlwaysOn)`，即跟随父 span。可通过 `SYNTHIA_OTEL_SAMPLER` 环境变量覆盖（`always_on` / `always_off` / `trace_id_ratio`）。

- **Q10：exporter 批处理配置？** 自行决议：默认 batch（5s interval / 512 batch size），与现有 gRPC 配置一致。
