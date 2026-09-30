## ADDED Requirements

### Requirement: Agent runtime SHALL 在 6 个关键路径边界创建 span

启用 `otel` feature 时，Agent runtime MUST 在以下 6 个边界创建 OTel span：
1. `session.start` / `session.end` — `Agent::run_stream` 入口/出口，作为 session 级 root span
2. `turn.start` / `turn.end` — `LoopContext` 每次 turn 迭代入口/出口
3. `llm.call` — `synthia-llm` provider 调用前后
4. `tool.execute` — `synthia-tool` registry 执行前后
5. `compaction` — `synthia-context` compaction 触发
6. `guardian.check` — `synthia-guardian` reviewer 调用

所有 span 创建代码 MUST 置于 `#[cfg(feature = "otel")]` 守卫下。无 `otel` feature 时，对应位置 SHALL 无 span 创建开销（编译期消除）。

#### Scenario: 6 个 span 边界均受 cfg 守卫

- **WHEN** 在 `synthia-agent` / `synthia-llm` / `synthia-tool` / `synthia-context` / `synthia-guardian` 源码中搜索 span 创建代码（`tracing::span!` / `tracing::instrument` / `OtSpanContext` 相关）
- **THEN** 每处 span 创建 SHALL 位于 `#[cfg(feature = "otel")]` 守卫的模块、函数或块内

#### Scenario: 无 feature 时无 span 开销

- **WHEN** 在未启用 `otel` feature 的构建中运行 `Agent::run_stream`
- **THEN** 编译产物中 SHALL 不存在 span 创建相关代码；运行时 SHALL 无 span 创建开销

---

### Requirement: session span SHALL 作为 root span 并跨越整个 run_stream

`Agent::run_stream` 入口 MUST 创建名为 `session.start` 的 span（kind: `Server` 或 `Internal`），并跨越整个 `run_stream` 调用周期。该 span SHALL 作为 root span（无 parent）。span MUST 记录 `session.id` 与 `user.id` 属性（若 `SpanAttributesProcessor` 已装配则自动注入，否则手动 `span.set_attribute`）。`run_stream` 正常返回或 panic 时，span SHALL 被 end（通过 RAII guard 或 `Drop` 实现）。

#### Scenario: run_stream 入口创建 session span

- **WHEN** 启用 `otel` feature 且 `Agent::run_stream` 被调用
- **THEN** SHALL 在入口创建名为 `session.start` 的 span，该 span 跨越整个 `run_stream` 执行周期

#### Scenario: session span 在 panic 时也被 end

- **WHEN** `Agent::run_stream` 在执行过程中 panic
- **THEN** `session.start` span SHALL 仍被 end（通过 RAII guard 的 `Drop` 实现），且 span status SHALL 标记为 `Error`

---

### Requirement: turn span SHALL 在每次 turn 迭代创建

`LoopContext` 的 turn 迭代入口 MUST 创建名为 `turn.start` 的 span（kind: `Internal`），parent 为当前 `session.start` span。span MUST 记录 `turn.id` / `turn.iteration` 属性。turn 正常完成时 span 被 end；turn 失败（如 LLM 调用错误）时 span status 标记为 `Error` 并记录 `exception` 事件。

#### Scenario: 每次 turn 创建独立 span

- **WHEN** `Agent::run_stream` 执行 N 次 turn 迭代
- **THEN** SHALL 创建 N 个独立的 `turn.start` span，每个 span 的 parent 为 `session.start` span

#### Scenario: turn 失败时记录 exception 事件

- **WHEN** turn 迭代中 LLM 调用或工具执行失败
- **THEN** `turn.start` span SHALL 标记 status 为 `Error`，并 SHALL 记录 OTel `exception` 事件（含 `exception.type` 与 `exception.message`）

---

### Requirement: llm.call span SHALL 记录 provider 与 token usage

`synthia-llm` provider 调用 MUST 创建名为 `llm.call` 的 span（kind: `Client`），parent 为当前 `turn.start` span。span MUST 记录 `gen_ai.system` / `gen_ai.request.model` / `gen_ai.response.finish_reason` 属性，以及 `gen_ai.usage.input_tokens` / `gen_ai.usage.output_tokens` 属性（来自 `Usage` 结构）。调用失败时记录 `exception` 事件。

#### Scenario: LLM 调用创建带 usage 的 span

- **WHEN** `synthia-llm` provider 被调用并成功返回
- **THEN** `llm.call` span SHALL 包含 `gen_ai.system` / `gen_ai.request.model` / `gen_ai.usage.input_tokens` / `gen_ai.usage.output_tokens` 属性，且 span status 为 `Unset` 或 `Ok`

#### Scenario: LLM 调用失败记录 exception

- **WHEN** `synthia-llm` provider 调用返回错误
- **THEN** `llm.call` span SHALL 标记 status 为 `Error`，并 SHALL 记录 `exception` 事件（含 `exception.type` 与 `exception.message`）

---

### Requirement: tool.execute span SHALL 记录工具名与执行时长

`synthia-tool` registry 的工具执行 MUST 创建名为 `tool.execute` 的 span（kind: `Internal`），parent 为当前 `turn.start` span。span MUST 记录 `tool.name` 属性（来自 `Tool::name()`）。执行失败或超时时，span status 标记为 `Error` 并记录 `exception` 事件。

#### Scenario: 工具执行创建带工具名的 span

- **WHEN** `ToolRegistry::execute` 被调用执行工具 `T`
- **THEN** `tool.execute` span SHALL 包含 `tool.name = T::name()` 属性

#### Scenario: 工具超时记录 exception

- **WHEN** 工具执行超时（触发 `tokio::time::timeout`）
- **THEN** `tool.execute` span SHALL 标记 status 为 `Error`，并 SHALL 记录 `exception` 事件（`exception.type = "TimeoutError"`）

---

### Requirement: compaction span SHALL 记录压缩前后 token 数

`synthia-context` 的 compaction 触发 MUST 创建名为 `compaction` 的 span（kind: `Internal`）。span MUST 记录 `compaction.before_tokens` / `compaction.after_tokens` / `compaction.stage` / `compaction.messages_before` / `compaction.messages_after` 属性。

#### Scenario: compaction 创建带前后 token 数的 span

- **WHEN** `synthia-context` 的 compaction 被触发并完成
- **THEN** `compaction` span SHALL 包含 `compaction.before_tokens` / `compaction.after_tokens` / `compaction.stage` 属性，反映压缩前后的 token 数与触发的 pruning stage

---

### Requirement: guardian.check span SHALL 记录决策结果

`synthia-guardian` 的 reviewer 调用 MUST 创建名为 `guardian.check` 的 span（kind: `Internal`）。span MUST 记录 `guardian.decision` 属性（值为 `allow` / `deny` / `need_user_confirm`）与 `guardian.layer` 属性（值为 `simple` / `reviewer` / `circuit_breaker`）。

#### Scenario: Guardian 审查创建带决策的 span

- **WHEN** `GuardianReviewer::check` 被调用并返回决策
- **THEN** `guardian.check` span SHALL 包含 `guardian.decision` 属性（值为 `allow` / `deny` / `need_user_confirm`）与 `guardian.layer` 属性

---

### Requirement: span 创建 SHALL 不修改 prompt 前缀

所有 span 创建 MUST 是旁路观测行为：不修改 `Agent::run_stream` 的 prompt 构造、不修改 `messages` 数组、不修改 `CompletionRequest` 的 system / tools / messages 字段。span 属性注入通过 OTel `Span::set_attribute` 完成，与 LLM 请求体完全解耦。此约束对应 P1 前缀一致性原则。

#### Scenario: span 创建不修改 messages

- **WHEN** `Agent::run_stream` 在启用 `otel` feature 时执行，且创建多个 span
- **THEN** 传给 `CompletionRequest` 的 `messages` / `system` / `tools` 字段 SHALL 与未启用 `otel` feature 时完全一致（字节级）

#### Scenario: span 创建不修改 prompt_cache_key

- **WHEN** `Agent::run_stream` 在启用 `otel` feature 时执行
- **THEN** `prompt_cache_key` 的计算输入（`user_id` / `session_id`）SHALL 与未启用时一致，确保 KV cache 命中率不受 span 创建影响
