## Purpose

`SpanAttributesProcessor` 自动从 `SystemContext` 与 `AgentRunContext` 提取并注入标准 span 属性（session.id / turn.id / agent.id / user.id / gen_ai.*），消除每个 `#[tracing::instrument]` 手写属性的 DRY 违规，并确保上下文缺失时 graceful skip。

## Requirements

### Requirement: SpanAttributesProcessor SHALL 实现 `SpanProcessor` trait

`synthia-telemetry` SHALL 新增 `SpanAttributesProcessor`（位于 `crates/synthia-telemetry/src/span/attributes_processor.rs`），实现 `opentelemetry_sdk::trace::SpanProcessor` trait。该 processor MUST 在 `on_start` 钩子中从当前 `tracing::Span::current()` 与 `tokio::task_local` 上下文提取 `SystemContext`（P1-4 Source/Epoch）和 `AgentRunContext`，并注入标准 span 属性。整个实现 MUST 置于 `#[cfg(feature = "otel")]` 下。

#### Scenario: SpanAttributesProcessor 实现 SpanProcessor trait

- **WHEN** 检查 `span/attributes_processor.rs`
- **THEN** SHALL 存在 `pub struct SpanAttributesProcessor` 并 `impl SpanProcessor for SpanAttributesProcessor`，覆盖 `on_start` / `on_end` / `force_flush` / `shutdown` 方法

#### Scenario: 模块受 otel feature 守卫

- **WHEN** 在 `span/mod.rs` 中检查 `attributes_processor` 模块声明
- **THEN** 该模块声明 SHALL 位于 `#[cfg(feature = "otel")]` 守卫下

---

### Requirement: Processor SHALL 注入 6 个标准 span 属性

`SpanAttributesProcessor` 在 `on_start` 时 SHALL 尝试注入以下 6 个属性到 span：
- `session.id` — 当前 session ID（来自 `SystemContext`）
- `turn.id` — 当前 turn ID（来自 `AgentRunContext` 或 `LoopContext`）
- `agent.id` — agent 实例 ID
- `user.id` — 用户 ID（来自 `SystemContext` Source/Epoch，P1-4）
- `gen_ai.system` — LLM provider 名称（"anthropic" / "openai" / 等）
- `gen_ai.request.model` — 模型名称（如 "claude-3-5-sonnet-20241022"）

属性注入 SHALL 使用 OTel 语义约定（`opentelemetry-semantic-conventions` crate 的常量）。若任一上下文缺失，processor SHALL graceful skip 该属性（不报错、不 panic）。

#### Scenario: 完整上下文时注入全部 6 个属性

- **WHEN** `SpanAttributesProcessor::on_start` 被调用，且当前 `tracing::Span::current()` 或 task-local 上下文中存在完整的 `SystemContext` 与 `AgentRunContext`
- **THEN** span SHALL 包含 `session.id` / `turn.id` / `agent.id` / `user.id` / `gen_ai.system` / `gen_ai.request.model` 共 6 个属性

#### Scenario: 上下文缺失时 graceful skip

- **WHEN** `SpanAttributesProcessor::on_start` 被调用，但 `SystemContext` 或 `AgentRunContext` 不可达（如 standalone 测试、未装配 processor 上下文）
- **THEN** processor SHALL 不注入对应属性，且 SHALL 不 panic、不返回错误、不记录 ERROR 日志（INFO 级别日志可选）

#### Scenario: 使用 OTel 语义约定常量

- **WHEN** 检查 `attributes_processor.rs` 中属性 key 的写法
- **THEN** `session.id` / `user.id` 等属性 key SHALL 优先使用 `opentelemetry_semantic_conventions` crate 提供的常量（如 `SESSION_ID` / `USER_ID` / `GEN_AI_SYSTEM` / `GEN_AI_REQUEST_MODEL`），而非硬编码字符串

---

### Requirement: Processor SHALL 通过 `tracing` span 扩展或 task-local 获取上下文

`SpanAttributesProcessor` SHALL 按以下优先级提取上下文：
1. **优先**：`tracing::Span::current()` 的 extensions 中查找 `SystemContext` / `AgentRunContext`（若 P1-4 已通过 `tracing::Value` 暴露）
2. **次选**：`tokio::task_local` 变量（在 `Agent::run_stream` 入口注入）
3. **fallback**：graceful skip

`Agent::run_stream` 入口 MUST 在启用 `otel` feature 时通过 `task_local` 或 `tracing::span!` 注入 `SystemContext` 与 `AgentRunContext`，使 processor 可达。

#### Scenario: Agent::run_stream 注入上下文

- **WHEN** 启用 `otel` feature 且 `Agent::run_stream` 被调用
- **THEN** SHALL 在入口处通过 `task_local!` 或 `tracing::Span::current()` 注入 `SystemContext` 与 `AgentRunContext`，使其在子 span 的 `on_start` 中可达

#### Scenario: task-local 优先级低于 tracing span 扩展

- **WHEN** `tracing::Span::current()` 的 extensions 中存在 `SystemContext` 且 `task_local` 也存在
- **THEN** processor SHALL 优先使用 tracing span extensions 中的值（更精确，对应特定 span 的上下文）

---

### Requirement: SpanAttributesProcessor SHALL 装配到 tracer provider

`init_otlp_tracing` 函数 SHALL 将 `SpanAttributesProcessor` 作为 `SpanProcessor` 装配到 `SdkTracerProvider`（通过 `with_span_processor`）。装配 MUST 在 exporter 装配之后、provider `build()` 之前。多个 processor（如内置 batch processor + SpanAttributesProcessor）SHALL 通过 `BatchSpanProcessor::builder().add_span_processor()` 或等价方式组合。

#### Scenario: provider 装配 SpanAttributesProcessor

- **WHEN** `init_otlp_tracing` 构造 `SdkTracerProvider`
- **THEN** 该 provider 的 builder SHALL 调用 `with_span_processor(SpanAttributesProcessor::new())` 或等价装配方法

#### Scenario: SpanAttributesProcessor 与 batch processor 共存

- **WHEN** tracer provider 初始化完成
- **THEN** span SHALL 同时经过 `SpanAttributesProcessor`（属性注入）和 batch processor（异步导出），两者不冲突

---

### Requirement: SpanAttributesProcessor SHALL 不修改 span 的语义

processor 的 `on_start` 仅注入属性，MUST NOT 修改 span 的 name、status、kind、parent 或其他语义字段。`on_end` 钩子 MUST 为 no-op（不读取、不修改 span）。`force_flush` 与 `shutdown` MUST 委托给被装饰的 inner processor（若采用装饰器模式）或为 no-op（若采用独立 processor）。

#### Scenario: on_start 仅注入属性

- **WHEN** `SpanAttributesProcessor::on_start` 被调用
- **THEN** 该方法 SHALL 仅通过 `span.set_attribute(...)` 注入属性，MUST NOT 调用 `span.update_name(...)` / `span.set_status(...)` 或修改其他语义字段

#### Scenario: on_end 为 no-op

- **WHEN** `SpanAttributesProcessor::on_end` 被调用
- **THEN** 该方法 SHALL 立即返回，不读取、不修改 span 数据
