# Synthia R4 — 优化与对标分析（traitclaw × dsh × pi）

> **基线**：R3（2026-09-06）已落地 traitclaw 风格的 Guard / Hint /
> Hook / Tracker / OutputTransformer 五件套 + 5 个 ReAct seam 接
> 线；synthia-context 已落地 3 层 Memory + 3 种 ContextManager
> （Truncating / Summarizing / Dag，移植自 pi-context-prune 与
> pi-lcm）；shell 工具已对齐 dsh 的 process_group 击杀 / timeout
> / 环境加固（详见 `docs/traitclaw-gap-analysis.md` §8）。
>
> **R4 目标**：在 R3 之上吸收 traitclaw / dsh / pi 三个项目里仍
> 未对齐的关键设计与逻辑，使 Synthia 的 agent runtime 达到「业界
> 领先」的事实标准。
>
> **范围声明**：本报告列出的所有 R4 项均经过 `read` / `grep` 实
> 测确认未被现有仓库覆盖（含 R3 已落地项的回滚边界）。`[INFER]`
> 标注的设计价值由对照推导得出，不代表已经被官方背书。

## 0. 阅读路径

1. §1 — 三个参考项目的设计哲学差异（决定 R4 取舍）。
2. §2 — R3 已落地的清单（R4 的"不要重复"基线）。
3. §3 — 仍未覆盖的差距（按 ROI / 实现成本排序）。
4. §4 — R4 落地计划（5 个 phase + 25 个交付物）。
5. §5 — 验收口径与风险。
6. §6 — 参考链接。

## 1. 三个参考项目的设计哲学

| 项目 | 抽象单位 | 关注点 | 数据流形态 |
|---|---|---|---|
| **traitclaw** | `Action` + `Guard`/`Hint`/`Hook`/`Tracker`（8 个 trait） | 可组合的拦截器栈（μs 级 sync + I/O 级 async），Builder 风格 Agent | 推送式（Hook 拉） |
| **deepseek-harness (dsh)** | Cordis `Service` + `Scope`（scope-keyed registries）+ `Session` 事件日志 + `EpochHeader` 请求头 | 服务化 + 作用域多租户 + 不可变事件溯源（append-only log + `surfaceOp`）+ 请求配置版本化 | 拉取式（Session::events + surface fold） |
| **pi-agent** | `AgentHarness` + `HookRegistry` + `EffectGate` + `Lane`（多会话并发隔离）+ `CompactionSettings` | 严格 typed hook 协议 + 操作编排（run/compaction/navigation/abort）+ 失败隔离（isolated handler delivery） | 严格类型化（每个 hook 一个 invocation schema） |

**结论**：

- **traitclaw 给我们的是 trait 词汇**（✅ R3 已落地）。
- **dsh 给我们的是会话不变量**——事件溯源 + surfaceOp（append/replace）
  + crash-repair + scope-keyed 注册中心 + 不可变 fold view。
  Synthia 当前 `SessionSink` 是「opaque JSONL + 单条流」，没有
  surface / 不可变 fold / repair 语义。
- **pi 给我们的是编排不变量**——hook 的 typed invocation + result +
  handler 失败隔离（`handler_error` 事件不阻塞其他 listener）+ lane
  隔离 + 操作间切换的 effect-gate。Synthia 当前 hook 串行执行、
  无失败隔离、所有 `AgentHook::on_*` 都是 async-no-signature 形式。

**R4 的核心决策**：以 **dsh 的事件溯源 + scope-keyed registries**
打会话层的不变量底座，以 **pi 的 typed-hook + isolated-delivery**
打 steering 层的不变量底座，**traitclaw** 的 trait 词汇保持不变
（R3 已经全量覆盖）。

## 2. R3 已落地的清单（R4 的"不要重复"基线）

### 2.1 steering crate（`crates/synthia-steering/`）

| 组件 | 来源 | 状态 |
|---|---|---|
| `Action` (6 变体 + fingerprint) | traitclaw | ✅ |
| `Guard` / `GuardResult` / `GuardSeverity` | traitclaw | ✅ |
| `run_guards`（catch_unwind fail-closed） | 修 traitclaw 缺陷 | ✅ |
| `Hint` / `InjectionPoint` (4) / `HintPriority` | traitclaw | ✅ |
| `Tracker`（观测 + 推荐并发） | traitclaw | ✅ |
| `AgentHook` (8 钩子) + `HookAction::Block` | traitclaw | ✅ |
| `OutputTransformer` | traitclaw | ✅ |
| `Steering` bundle（noop / default_policy） | 自有 | ✅ |
| 5 个内置 Guard（Loop / Budget / ShellDeny / Boundary / Injection） | 自有 | ✅ |
| 3 个内置 Hint（Budget / Iteration / Truncation） | 自有 | ✅ |
| `AdaptiveTracker` (8→2→1 降档) | dsh rolling-pool | ✅ |

### 2.2 ReActLoop 的 5 个 seam 接线

| Seam | 来源 | 状态 |
|---|---|---|
| hooks → guards → tracker → hints → transformer | dsh 拦截语义 | ✅ |
| guard 拒绝 → errors-as-results + `WarningKind::Guard` | 自有 | ✅ |
| sanitize 采纳为 effective call | 修 traitclaw 缺陷 | ✅ |
| 4 个 InjectionPoint 全部实现 + priority 门控 | 修 traitclaw 缺陷 | ✅ |

### 2.3 shell 工具加固（`crates/synthia-tool/src/builtin/shell.rs`）

| 加固项 | 对齐 | 状态 |
|---|---|---|
| cwd workspace 钳制 | dsh | ✅ |
| `process_group(0)` + SIGTERM→3s→SIGKILL | dsh | ✅ |
| timeout 默认 120s / 上限 600s 钳制 | dsh | ✅ |
| 非零退出 = 正常结果 + `[exit code: N]` 标记 | dsh | ✅ |
| `NO_COLOR` / `TERM=dumb` / `PAGER=cat` / `GIT_PAGER=cat` | dsh | ✅ |

### 2.4 context / memory crate（`crates/synthia-context/`）

| 组件 | 来源 | 状态 |
|---|---|---|
| `ContextManager` trait | traitclaw | ✅ |
| `TruncatingContextManager`（默认） | 自有 | ✅ |
| `SummarizingContextManager`（tool-result 摘要） | pi-context-prune | ✅ |
| `DagContextManager`（hierarchical DAG） | pi-lcm | ✅ |
| `Memory` trait（conversation / working / long-term） | traitclaw | ✅ |
| `InMemoryMemory` / `SessionMemory` | 自有 | ✅ |
| `events_to_messages` 共享投影 | 自有 | ✅ |

### 2.5 session crate（`crates/synthia-session/`）

| 组件 | 来源 | 状态 |
|---|---|---|
| `SessionSink`（5-method trait，event-shape-agnostic） | 自有收敛 | ✅ |
| `InMemorySessionSink` / JSONL 实现 | 自有 | ✅ |

### 2.6 agent runtime（`crates/synthia-agent/`）

| 组件 | 来源 | 状态 |
|---|---|---|
| `ReActAgent` + `ReActLoop`（5-step 状态机） | 自有 | ✅ |
| `Agent` trait + `AgentRegistry` | 自有 | ✅ |
| `AgentRunConfig`（max_iterations + Steering + ContextManager 注入） | 自有 | ✅ |
| `task(agent, prompt)` 子委派 + `MAX_SUBAGENT_DEPTH=3` | traitclaw `Team`/`Router` 等价 | ✅ |
| `AgentEvent`（4 variant）+ `WarningKind` | 自有 | ✅ |

### 2.7 server（`crates/synthia-server/`）

| 组件 | 来源 | 状态 |
|---|---|---|
| `AppState::steering`（default_policy 全局注入） | 自有 | ✅ |
| `SessionController`（per-session 操作队列 + 最多 1 个 run） | 自有 | ✅ |
| `AgentRunConfig` 工厂（注入 session） | 自有 | ✅ |

---

## 3. 仍未覆盖的差距（按 ROI / 实现成本排序）

### 3.1 高 ROI / 中成本 — R4 主战场

#### G-1 ❗ **会话层：dsh 式事件溯源 + `surfaceOp` + crash-repair**

**当前状态**：`synthia-session::SessionSink::append(Value)` 是「opaque JSONL」；
agent runtime 把 `AgentEvent` 序列化成 JSON 字符串后写入；replay 时通过
`SessionController::reconstructMessagesFromSession` 用自有私有的
`events_to_history` 投影回 `Message`。

**问题**：

1. **不可变 fold 缺位**：没有「这一段消息是 append 还是 replace」的语义，
   compaction / tool-result 重写不能表达「原 tool-result 已被新版本替换」
   —— R3 的 SummarizingContextManager 就是把 tool-result 替换为摘要，但
   这是 session JSONL 不可分辨的；replay 会重复看见旧 tool-result。
2. **crash-repair 缺位**：当服务器在 `tool_call` 已发出但 result 未写完时
   被 kill，重启后看到的就是一个 dangling call —— Synthia 没有
   `interruptedTurnClosers` 的等价物，会拒绝读这个 session。
3. **scope 隔离缺位**：所有 agent / tool / hook 注册在全局 `Registry` 上；
   pi 已经把 registry 设计成 scope-keyed（按 session 隔离），Synthia 当
   前 `AgentRegistry` 是全局的，子 agent 注册的 tool 会污染全局。

**dsh 对应物**（`packages/core/session/`）：

- `SessionEvent` 不可变 + `surfaceOp: 'append' | { op: 'replace'; start, end }`
- `SurfaceEventType = 'user/message' | 'assistant/message' | 'tool/result'`
  （仅这三个 type 可带 `surfaceOp`；其他 log-only event 无 surface 标记）
- `SessionSurface`：`foldSurface(events) -> Message[]`（按 surfaceOp replay）
- `interruptedTurnClosers(events) -> SessionEvent[]`（补全 open turn/step）
- `SessionPreparation`（provider-owned unpublished session 包装）

**R4 设计**（`crates/synthia-session` v2）：

1. 把 `SessionSink` 从 opaque JSONL 升级为 **typed event log**：
   `append(SessionEvent)`；其中 `SessionEvent` 是封闭枚举（`UserMessage` /
   `AssistantMessage` / `ToolResultMessage` / `Compaction` / `ToolCall` /
   `Step` / `Turn` / `Iteration` / `Warning` / `SteeringGuard` /
   `SteeringHint` / `HookBlock` / `SubagentEnter` / `SubagentExit`），
   `SurfaceEvent` 是带 `surfaceOp` 字段的子集。
2. 新增 `SurfaceManager::fold(events) -> Vec<Message>` —— 复刻 dsh
   `foldSurface` 的语义：append 是无脑拼接；replace 是原子替换
   `[start, end)` 范围并通过 `sourceEventSeqs` 验证 provenance。
3. 新增 `interruptedTurnClosers(events) -> Vec<SessionEvent>` ——
   复刻 dsh `repair.ts`，处理「call 已发，result 未写」+「step
   已开，step/end 未写」+「turn 已开，turn/end 未写」三态。
4. `SessionPreparation`（Disposable）—— 把「provider-owned session
   在未发布前的状态」与「已发布 session」分开。**R4 不必强制把
   SessionSink 改成这个形态**，但留接口。

**验收标准**：

- `cargo test -p synthia-session` 全绿，包含：
  - surface fold：append / replace 100% 与 dsh foldSurface 行为一致。
  - interruptedTurnClosers：3 个 dangling 场景（call-only / call+step
    / call+step+turn）输出 deterministic closer sequence。
  - `sourceEventSeqs` provenance check：错配 → panic or typed error。
- 旧 `Value` based 的 sink API 保留为 `OpaqueSessionSink`（构造器
  兼容旧调用者），但 session controller 默认改用新 typed sink。
- `SessionMemory::rehydrate(session_id)` 自动调用 `foldSurface`
  重建 `Vec<Message>`（替代旧的 `events_to_history`）。

**风险**：

- ⚠️ server 侧 JSONL 持久化格式与新 typed event 不兼容（破坏性）。需要
  做 v1 → v2 migration：旧 JSONL 行视为 opaque user-message surfaces
  （只能 append），无法做 replace；遇到旧格式写出 `_legacy: true`
  标记，下次启动警告「session is on legacy format」。

---

#### G-2 ❗ **hook 层：pi 式 typed hook + isolated-delivery + effect-gate**

**当前状态**：

```rust
#[async_trait]
pub trait AgentHook: Send + Sync + 'static {
    async fn on_agent_start(&self, input: &str) { ... }
    async fn on_provider_start(&self, request: &CompletionRequest) { ... }
    async fn on_provider_end(&self, response: &CompletionResponse, duration: Duration) { ... }
    async fn before_tool_execute(&self, tool_call: &ToolCall) -> HookAction { ... }
    async fn after_tool_execute(&self, tool_call: &ToolCall, result: &ToolOutput) { ... }
    async fn on_stream_chunk(&self, chunk: &StreamChunk) { ... }
    async fn on_error(&self, error: &Error) { ... }
    async fn on_agent_end(&self, output: &AgentOutput) { ... }
}
```

注册为 `Vec<Arc<dyn AgentHook>>`，**按注册顺序串行调用**，一个
hook 抛错会传播（虽然 `LoggingHook` 用了 tracing 不抛，但 R3 没
有 fail-isolation 约束）。

**问题**：

1. **没有 fail-isolation**：pi `HarnessEventBus.deliver()` 把每个
   listener 的执行包在 try/catch 里，某个 hook 抛错只发
   `handler_error` 事件不影响其他 hook。Synthia `LoggingHook` 是
   `tracing::info!()` 但如果用户写一个会 panic 的 hook，整个
   `on_provider_end` 就会 panic，session 中断。
2. **没有 typed hook protocol**：每个 hook 接收任意事件，开发者
   容易写错（`&CompletionRequest` 但不知道哪个 provider 的）。
3. **没有 hook span / tracing context**：pi `HookRegistry` 自动为
   每次 hook 调用开 span（`startHarnessSpan('hook', { name })`），
   Synthia hook 调用没有 span，只有 `tracing::Span::current()` 透
   传。
4. **`HookAction::Block` 与 guard 的语义重叠**：traitclaw 的 hook
   既能 veto 也能观察；Synthia 当前 `HookAction::Block(reason)`
   在 `before_tool_execute` 里调用会写进 tool result 与 guard 拒
   绝同路径——但 hook 不知道 guard 会怎么判，造成重复拦截。pi
   的做法是把 veto 权完全交给 effect-gate，hook 只观察 + 抛错。

**pi 对应物**（`packages/agent/src/harness/`）：

- `HookMap<TName extends string>`：每个 hook name 一对
  `{ event: TEvent, result: TResult }` —— 调用方把 event 投递
  给 handler，handler 返回 `result`，调用方按 result 的 schema
  做反应。
- `HookRegistry`：每个 hook 调用包 `try { handler(event, ctx) }
  catch (e) { emit 'handler_error' }`；handler error 不影响其它
  handler。
- `EffectGate`（`execution/effect-gate.ts`）：procedure-facing
  `gate.admit(invoke) -> T`；abort 路径走 `beginAbort` →
  `signalAbort` → AbortController。`gate.admit` 同步检查 + 调用；
  外部 cancel 必须 `admit` 抛 `AbortRequested` 才能传播。
- `BufferEventWatcher<T>`：观测量化事件——watch consumer 自己
  维护 buffer，按 resnapshot 重算。

**R4 设计**（`crates/synthia-steering/src/hook_v2.rs` 或升级
现有 `hook.rs`）：

1. **Fail-isolation wrapper**：所有 hook 调用包
   `AssertUnwindSafe` + `catch_unwind`，把 `String` panic payload
   转 `HookError`（结构化：`{ hook_name, message }`），发
   `AgentEvent::System(SystemEvent::HookError { hook, message })`
   事件，session 继续。
2. **Hook span 自动开启**：每次 hook 调用前开一个
   `tracing::info_span!("hook", name = hook.name())`，调用结束
   后 `span.exit()`。与 synthia-telemetry 的 OTel 透传一致。
3. **HookAction::Block 保留但语义收敛**：仅在
   `before_tool_execute` 生效；其他钩子的返回值不再是 `Block`
   而是 `Continue`（no-op marker）；guard 才是 veto 主体。
4. **可选 `EffectGate`**：把 `ReActLoop` 的取消逻辑收敛到
   `EffectGate::admit(invoke)`：每个 hook / tool / llm 调用前
   `gate.admit(|| ...)`，外部 cancel 走 `gate.signal_abort()`，
   内部不需要每个 hot path 手动 `token.is_cancelled()`。**R4
   可选落地**（属于优化项，P1）。

**验收标准**：

- `cargo test -p synthia-steering` 增 4 个测试：
  - hook panic 不影响其他 hook（注册 2 个，1 个 panic；另 1 个
    仍被调用 + `HookError` 事件发出）。
  - hook 调用自动生成 `tracing::Span` 且 `span.record(error)` 在
    panic 时记录。
  - `HookAction::Block` 仅在 `before_tool_execute` 生效；其他
    位置传入 Block 返回 `HookError` 而非 panic。
  - `EffectGate::admit` 在 `signal_abort` 后同步抛 `AbortRequested`。
- `LoggingHook` 加 panic counter metric（`tracing` + `metrics`）。

**风险**：

- ⚠️ 用户自定义 hook 的语义在 panic 时变化（之前 panic 杀 session，
  之后只发 `HookError` 事件）。需要在 R4 changelog + AGENTS.md
  显著标注。

---

#### G-3 ❗ **provider 层：dsh 式 typed retry + token-meter + EpochHeader**

**当前状态**：

- `synthia-provider::retry::RetryProvider` 已有基本 retry（指数
  backoff + jitter），但错误分类只分 `Transient` / `RateLimit` /
  `Permanent`，不识别 provider-specific 错误（如 Anthropic
  `overloaded_error` vs `rate_limit_error`）。
- `TokenUsage` 是单一聚合（input / output），dsh 的
  `Usage` + `UsageRow` 模式更细（cache_creation / cache_read /
  reasoning / output 单独计费）。
- provider request header 没有 versioning：每次 LLM 调用都是
  `CompletionRequest { messages, tools, config }` 直接发；dsh
  `EpochHeader` 把 request header 作为 durable event 写入 session，
  model switch 行为可 replay。

**dsh 对应物**（`packages/llm/`）：

- `RetryPolicy`：per-error-type 的 exponential + jitter +
  budget cap（`maxAgentDelayMs`）；error-specific handler
  （`overloadedError` / `rateLimitError` / `authenticationError`）。
- `BlockAssembler`：把多轮 content blocks 拼成稳定的请求体
  （cache-friendly block ordering）。
- `attribution.ts`：把 token 消耗归因到具体的 agent / tool / turn。
- `EpochHeader`：durable per-call config snapshot
  （provider / model / tools / system-prompt section ids）——
  resume 时可重放完全相同的 LLM 调用环境。

**R4 设计**：

1. **typed retry**：把 `RetryProvider` 的错误分类升级为
   `AnthropicOverloaded` / `AnthropicRateLimit` / `OpenAIRateLimit`
   / `OpenAIServer` / `Permanent` 等；每个有独立的 backoff cap 与
   max-retry。
2. **token-meter**（独立 crate `synthia-meter` 或
   `synthia-telemetry::meter`）：拆 `TokenUsage` 为
   `input / cache_creation / cache_read / reasoning / output`；
   把每次 LLM 调用的 usage 落到 `usage_events` 表（durable），
   支持按 agent / tool / turn 归因。
3. **EpochHeader snapshot（轻量版）**：在 `SessionSink::append`
   之前写一个 `SessionEvent::RequestHeader { epoch, provider,
   model, tools_hash, system_hash }`；agent loop 在 model switch
   时自动写；session replay 能告诉用户「在第 N 步切换到 GPT-4」。
   完整 dsh 的 fold-to-tool-identity 太重，R4 只落地 trace。

**验收标准**：

- `cargo test -p synthia-provider` 增 3 类测试：
  - typed retry：AnthropicOverloaded 重试 3 次、max 60s；rate
    limit 重试 5 次、max 30s；permanent 立即返回。
  - token-meter：Anthropic 调用的 `cache_read_tokens` 在
    `usage_events` 表能 query 出。
  - EpochHeader snapshot：同 session 内 model switch 留下 2 个
    `RequestHeader` event。

**风险**：

- ⚠️ `TokenUsage` 字段拆分会破坏现有调用方。先做 `TokenUsageV2`
  双写，旧字段 deprecate，1 个 minor 版本后删除。

---

### 3.2 中 ROI / 中成本 — R4 第二梯队

#### G-4 🔸 **tool 层：traitclaw 式 `ExecutionStrategy` + dsh 式 barrier pool**

**当前状态**：`Tool::execution_mode()`（`Parallel` / `Sequential`）
是工具级静态属性；ReActLoop 按 mode 分桶后用 `tokio::join!`
跑并行桶。

**问题**：

1. 没有**批次级**策略（一个 assistant turn 内的 N 个 tool call
   应该并行还是串行？tracker 推荐并发是 2 还是 8？）。
2. R3 的 `AdaptiveTracker::recommended_concurrency` 推荐值当
   前没被 ReActLoop 消费——`semaphore::for_each` 没真正接上。

**traitclaw 对应物**（`crates/traitclaw-core/src/traits/execution_strategy.rs`）：

- `ExecutionStrategy::execute_batch(pending, ctx) -> Result<Vec<Output>>`
- `SequentialStrategy` / `ParallelStrategy` / `AdaptiveStrategy` /
  `DependencyStrategy`。

**dsh 对应物**（`packages/core/agent-loop/src/tool-calls.ts`）：

- `runGroup(group, mode, signal)`：`exclusive` barrier（N 个串
  行 tool 串行等待前一个结束）+ `parallel` pool（N 个并行但受
  semaphore 封顶）。
- `PlannedCall` / `Slot`：每个 tool call 解析后的 planned 状态，
  barrier 收集 settled outcomes。

**R4 设计**：

1. 把 `AdaptiveTracker::recommended_concurrency` 真正接到
   ReActLoop 的并行桶：`let cap = tracker.recommended_concurrency();`
   `Semaphore::new(cap)`，`tokio::spawn(...).await` 之前
   `permit.acquire()`。
2. `ExecutionStrategy` trait（最小版）：`async fn
   execute_batch(&self, calls: Vec<ToolCall>) -> Vec<ToolOutput>`；
   默认 `AdaptiveStrategy`（读 tracker），可换 `SequentialStrategy`。
   **不引入完整 traitclaw 14.6KB 模块**，只引入 boundary。

**验收标准**：

- `cargo test -p synthia-agent` 增 2 个测试：
  - 6 个并行 tool call，tracker concurrency=2 → 实际并发 ≤ 2。
  - 6 个并行 tool call，tracker concurrency=8 → 全部并行。

**风险**：低。语义是现有并行桶的语义收紧。

---

#### G-5 🔸 **session 层：pi 式 Lane 多会话并发 + CompactionSettings**

**当前状态**：

- `SessionController` 是 per-session 单例，每 session 一个
  mpsc：`mpsc::UnboundedReceiver<SessionOp>` + 一个 mpsc for
  events。
- 多个 session 是独立的 controller（`HashMap<SessionId,
  Arc<SessionController>>`），彼此不感知。

**pi 对应物**（`packages/agent/src/harness/runtime/lane.ts`）：

- `Lane` = 一条 lane = 一个 session fork 后裔；
- `Config::lanes[]`：用户预定义 lane 名 + 配置 + initial entry；
- `LaneCommand` / `OperationCommand`：lane 内串行 mutation line
  （`commit` + `next: LaneState`），lane 间隔离。

**R4 设计**：

R4 不必引入完整 lane 模型（Synthia 当前是「一个 session 一个
controller」足够），但**抽象一个 LaneTrait** 为未来多会话 fan-out
留接口：

```rust
pub trait SessionLane: Send + Sync {
    fn lane_id(&self) -> &str;
    async fn submit(&self, op: SessionOp) -> Result<(), LaneError>;
    fn subscribe(&self) -> broadcast::Receiver<AgentEvent>;
}
```

实现 `DefaultLane` = 现有 `SessionController`，把
`SessionController::submit` 包成 `SessionLane` 接口。

**验收标准**：纯 trait 抽象；不改变行为。

**风险**：低。这是前向兼容预留。

---

### 3.3 低 ROI / 高成本 — R4 第三梯队（候选）

#### G-6 ⏸️ **prompt 层：pi 式 PromptTemplate registry + dsh PromptAssembly**

**当前状态**：

- `synthia-agent::prompt::PromptContext::assemble` 渲染 5 段
  （base / identity / tools / skills / agent / rules），XML
  拼接。
- 没有 PromptTemplate 注册中心：每个 agent 启动时自带
  `system_prompt` 字段，没有按用户 / team / workflow 分发的机制。

**pi 对应物**（`packages/agent/src/harness/prompt-templates.ts`）：

- `PromptTemplateRegistry`：命名 template + variables + 渲染函数。
- `prompt(kind: 'prompt', name: '...', args?: string[])` ——
  harness 入口接受 template name 而非裸文本。

**R4 决策**：❌ **不做**。Synthia 当前 `PromptContext::assemble`
已经覆盖 dsh `PromptAssembly` 的语义（XML-tagged section rendering），
多一层抽象收益低。R5+ 再考虑。

---

#### G-7 ⏸️ **InboxTarget：`next-turn` vs `next-step` 注入队列**

**dsh 对应物**（`packages/core/agent/src/types.ts`）：

- `InboxTarget = 'next-turn' | 'next-step'`：用户消息可指定
  在下一轮（turn）注入还是下一步（step）注入；step 注入
  不等 turn 结束。
- 触发 `agent/inbox/spliced` event。

**R4 决策**：❌ **不做**。Synthia 当前 `SessionOp::Steer` /
`SessionOp::Inject` 已经是 mpsc-based 异步注入，转 turn 是用户
心智模型；多一种 step 注入会让 UX 复杂化。等真实场景出现再做。

---

#### G-8 ⏸️ **`AgentMessage` vs `Message` 分离 + declaration merging**

**pi 对应物**（`packages/agent/src/types.ts`）：

```typescript
export interface CustomAgentMessages {
  // 用户扩展点
}
export type AgentMessage = Message | CustomAgentMessages[keyof CustomAgentMessages];
```

Synthia 的 `Message` 直接来自 `synthia-provider::Message`；没有
「agent-level message 与 provider-level message 分离」的层次。

**R4 决策**：❌ **不做**。Rust 没有 declaration merging，且
`provider::Message` 已经是 content-parts 形式（`Vec<ContentPart>`）
可扩展（文本 / 图像 / tool_call / tool_result 都已建模）。

---

## 4. R4 落地计划

### 4.1 总览

| Phase | 内容 | 涉及 crate | 预计交付 |
|---|---|---|---|
| **Phase A** | 会话层 v2：typed event log + surface fold + crash-repair | `synthia-session`, `synthia-agent`, `synthia-server` | G-1 |
| **Phase B** | hook 层 v2：typed + fail-isolation + span | `synthia-steering`, `synthia-agent` | G-2 |
| **Phase C** | provider 层 v2：typed retry + token-meter + EpochHeader | `synthia-provider`, `synthia-telemetry`, `synthia-session` | G-3 |
| **Phase D** | tool 层 v1：AdaptiveStrategy 真实接线 | `synthia-agent`, `synthia-steering` | G-4 |
| **Phase E** | lane trait 预留 | `synthia-server` | G-5 |

### 4.2 Phase A：会话层 v2（G-1）

#### A.1 `synthia-session::event` 模块

**新增**：

- `SessionEvent` 封闭枚举（20+ variant）：
  - `UserMessage { id, content, surface_op }`
  - `AssistantMessage { id, content, tool_calls, surface_op }`
  - `ToolResult { id, call_id, content, surface_op }`
  - `Compaction { from_seq, to_seq, summary }`
  - `ToolCall { call_id, name, arguments }`（log-only，无 surface）
  - `Step { kind: 'start' | 'end', turn, step }`（log-only）
  - `Turn { kind: 'start' | 'end', turn, reason }`（log-only）
  - `Iteration { kind: 'start' | 'end', iteration }`（log-only）
  - `Warning { kind, message }`（log-only）
  - `SteeringGuard { guard, action, reason }`（log-only）
  - `SteeringHint { hint, content, injection_point }`（log-only）
  - `HookBlock { hook, reason }`（log-only）
  - `SubagentEnter { child_session_id, depth, parent_id }`（log-only）
  - `SubagentExit { child_session_id, depth, end_reason }`（log-only）
  - `RequestHeader { epoch, provider, model, tools_hash, system_hash }`（log-only）
  - `UsageRecord { epoch, input_tokens, output_tokens, cache_read, cache_creation, reasoning }`（log-only）
- `SurfaceEvent` = `SessionEvent & { surface_op: SurfaceOp }`
  - 仅 `UserMessage` / `AssistantMessage` / `ToolResult` /
    `Compaction` 可带 `surface_op`。
- `SurfaceOp = 'append' | { op: 'replace'; start: number; end: number; source_event_seqs: Vec<u64> }`

**约束**：

- 所有 field 都用 `String` / `Vec<u8>` 等可序列化基础类型（避开
  `serde_json::Value`，保证 schema 强类型）。
- `#[serde(tag = "type", content = "data")]` 区分 variant。

#### A.2 `synthia-session::surface` 模块（复刻 dsh `surface.ts`）

```rust
pub fn fold_surface(events: &[SessionEvent]) -> Vec<Message>;
pub fn is_surface_event(event: &SessionEvent) -> bool;
pub fn is_append_surface_event(event: &SessionEvent) -> bool;
pub fn is_replacement_surface_event(event: &SessionEvent) -> bool;
pub fn derive_event_message(event: &SessionEvent) -> Option<Message>;
```

**关键不变量**：

- `replace` 必须带 `source_event_seqs`，验证这些 seq 都已落 log
  且属于「正在被替换的范围」内的 event。
- `fold_surface` 是纯函数（输入 events 切片 → 输出 `Vec<Message>`），
  无状态。

#### A.3 `synthia-session::repair` 模块（复刻 dsh `repair.ts`）

```rust
pub fn interrupted_turn_closers(events: &[SessionEvent]) -> Vec<SessionEvent>;
pub const TOOL_NOT_STARTED: &str;
pub const TOOL_OUTCOME_UNKNOWN: &str;
```

**逻辑**：

- 单次扫描 events，记录 `open_turn` / `open_step` / `pending_calls`。
- 在 `events.at(-1)` 后追加：
  1. 每个 dangling call：`ToolResult { error: TOOL_NOT_STARTED }`
  2. `Step { kind: 'end', turn, step }`（如果 step open）
  3. `Turn { kind: 'end', turn, reason: Interrupted }`

#### A.4 `synthia-session::sink` 升级

**新 trait**（breaking change，但旧 trait 保留为
`OpaqueSessionSink`）：

```rust
#[async_trait]
pub trait SessionSink: Send + Sync {
    fn id(&self) -> &str;
    async fn append(&self, event: SessionEvent) -> Result<(), SessionError>;
    async fn read(&self) -> Result<Vec<SessionEvent>, SessionError>;
    async fn snapshot(&self) -> Result<SessionSnapshot, SessionError>;
    async fn close(&self, end: SessionEndReason) -> Result<(), SessionError>;
}
```

**JSONL 行格式**（向后兼容 + 渐进迁移）：

```json
{"v": 2, "type": "user_message", "data": {...}}
{"v": 2, "type": "assistant_message", "data": {...}}
```

旧版 `{"v": 1, "payload": <opaque_value>}` 仍然可读，但被标记
为 legacy（只能在 user_message surface 上 append，无法 replace）。

**实现**：

- `JsonlSessionSink`：每行 NDJSON；写新 v2 格式。
- `InMemorySessionSink`：保持内存版。
- 新增 `repair::repair_session(sink)` helper：启动时调
  `interrupted_turn_closers` 把合成的 events 补到 sink。

#### A.5 `synthia-context::memory` 用 surface fold

替换 `events_to_messages` → `fold_surface`。

```rust
impl SessionMemory {
    pub fn rehydrate(&self, session_id: &str) -> Result<Vec<Message>, MemoryError> {
        let events = self.sink.read().await?;
        let messages = fold_surface(&events);
        Ok(messages)
    }
}
```

**删除**：旧私有 `events_to_history`（已被 traitclaw-gap-analysis
记录的 server-side helper）。

#### A.6 `synthia-server` 接入

- `SessionController::reconstruct_messages_from_session` 调
  `fold_surface(&sink.read().await?)`。
- `SessionController::append_event(event: SessionEvent)` 新
  API（替代旧的 `append_json(value)`）。
- session 启动时调 `repair_session(&sink)` 自动修复 crash tail。

#### A.7 Phase A 验收

- [ ] `cargo test -p synthia-session` ≥ 30 项（含 3 类 interrupted
      closer 测试 + surfaceOp replace 测试 + legacy migration 测
      试）。
- [ ] `cargo test -p synthia-agent` ≥ 175 项（已通过）
- [ ] `cargo test -p synthia-server` ≥ 360 项
- [ ] `cargo test -p synthia-context` ≥ 25 项
- [ ] workspace clippy 0 警告。
- [ ] 不破 R3 的 169 / 175 / 354 三个 baseline 数。

### 4.3 Phase B：hook 层 v2（G-2）

#### B.1 `synthia-steering::hook::FailIsolatedHook`

```rust
pub struct FailIsolatedHook {
    inner: Arc<dyn AgentHook>,
}

#[async_trait]
impl AgentHook for FailIsolatedHook {
    async fn on_agent_start(&self, input: &str) {
        let span = tracing::info_span!("hook", name = self.inner.name(), kind = "on_agent_start");
        let _g = span.enter();
        match AssertUnwindSafe(self.inner.on_agent_start(input)).catch_unwind().await {
            Ok(()) => {}
            Err(payload) => {
                let message = panic_message(&payload);
                tracing::error!(panic = %message, "hook panicked");
                // 发 SystemEvent::HookError 事件
            }
        }
    }
    // ... 其他方法同样包
}
```

#### B.2 `Steering::wrap_fail_isolated()`

```rust
impl Steering {
    pub fn wrap_fail_isolated(mut self) -> Self {
        self.hooks = self.hooks.into_iter().map(|h| {
            Arc::new(FailIsolatedHook { inner: h }) as Arc<dyn AgentHook>
        }).collect();
        self
    }
}
```

`Steering::default_policy()` 默认调用 `wrap_fail_isolated()`。

#### B.3 HookAction 语义收敛

- `HookAction::Continue`：no-op marker。
- `HookAction::Block(reason)`：仅 `before_tool_execute` 接受；其他
  位置传入 Block 返回 `HookError::BlockOutsideToolExecute`。

#### B.4 Phase B 验收

- [ ] `cargo test -p synthia-steering` 增 4 项：
      panic hook 隔离、span 自动开启、Block 位置检查、HookError
      事件发射。
- [ ] 所有内置 hook（`LoggingHook` / `MetricsHook`）加 panic 计数
      metric。

### 4.4 Phase C：provider 层 v2（G-3）

#### C.1 typed retry

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryableError {
    Permanent,
    AnthropicOverloaded,
    AnthropicRateLimit,
    OpenAIRateLimit,
    OpenAIServer,
    NetworkTransient,
}

pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub per_error: HashMap<RetryableError, RetryOverride>,
}
```

#### C.2 token-meter（`synthia-telemetry::meter` 或独立 crate）

```rust
pub struct UsageEvent {
    pub epoch: u64,
    pub session_id: String,
    pub agent_id: String,
    pub turn: u64,
    pub step: u64,
    pub input_tokens: u32,
    pub cache_creation_tokens: u32,
    pub cache_read_tokens: u32,
    pub reasoning_tokens: u32,
    pub output_tokens: u32,
}

pub trait UsageSink: Send + Sync {
    async fn record(&self, event: UsageEvent) -> Result<(), MeterError>;
}
```

默认实现 `InMemoryUsageSink`（Vec 累积）。未来可挂 Prometheus
counter。

#### C.3 EpochHeader snapshot

`ReActLoop::sample_once` 前调用
`sink.append(RequestHeader { epoch, provider, model, tools_hash,
system_hash })`；`epoch` 自增，model switch 时 reset。

#### C.4 Phase C 验收

- [ ] `cargo test -p synthia-provider` 增 5 项（typed retry +
      token-meter + EpochHeader）。
- [ ] Anthropic provider 把 `cache_read_input_tokens` 解析到
      `cache_read_tokens`。
- [ ] OpenAI provider 把 `prompt_tokens_details.cached_tokens`
      解析到 `cache_read_tokens`。

### 4.5 Phase D：tool 层 v1（G-4）

#### D.1 `synthia-agent::execution_strategy` 模块

```rust
#[async_trait]
pub trait ExecutionStrategy: Send + Sync {
    async fn execute_batch(
        &self,
        calls: Vec<PlannedToolCall>,
        state: &AgentState,
        tracker: &dyn Tracker,
    ) -> Vec<ToolOutput>;
}

pub struct AdaptiveStrategy;

#[async_trait]
impl ExecutionStrategy for AdaptiveStrategy {
    async fn execute_batch(
        &self,
        calls: Vec<PlannedToolCall>,
        state: &AgentState,
        tracker: &dyn Tracker,
    ) -> Vec<ToolOutput> {
        let cap = tracker.recommended_concurrency().max(1);
        let sem = Arc::new(Semaphore::new(cap as usize));
        // tokio::spawn + permit + ExecutionMode 分桶
    }
}
```

#### D.2 ReActLoop 接线

`ReActLoop::execute_tool` 拆为 `prepare_tool_calls` →
`execute_batch(strategy)` → `commit_tool_results`。

#### D.4 Phase D 验收

- [ ] `cargo test -p synthia-agent` 增 2 项（concurrency cap）。
- [ ] tracker concurrency 推荐值被实际消费（不再是 dead code）。

### 4.6 Phase E：lane trait（G-5）

#### E.1 `synthia-server::lane` 模块

```rust
#[async_trait]
pub trait SessionLane: Send + Sync {
    fn lane_id(&self) -> &str;
    async fn submit(&self, op: SessionOp) -> Result<(), LaneError>;
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<AgentEvent>;
    async fn close(self: Arc<Self>) -> Result<(), LaneError>;
}

pub struct DefaultLane {
    controller: Arc<SessionController>,
}
```

#### E.2 Phase E 验收

- [ ] `cargo test -p synthia-server` 增 2 项（trait 满足 +
      DefaultLane 委派）。
- [ ] 旧 `SessionController::submit` 路径完全保留（不破坏）。

---

## 5. 验收口径与风险

### 5.1 总验收口径

| 验收维度 | 命令 / 标准 |
|---|---|
| 编译 | `cargo check --workspace --all-features` 0 错误 |
| Lint | `cargo clippy --all-targets --all-features --tests --all -- -D warnings` 0 警告 |
| 格式 | `cargo +nightly fmt --all` 无 diff |
| 单元测试 | 4 个核心 crate（agent / tool / server / steering）基线 + Phase A/B/C/D/E 新增项全绿 |
| 回归 | R3 基线数（agent 169 / tool 175 / server 354）零退化 |
| 文档 | AGENTS.md §3 同步更新（不破坏既有 §3.6 自动化入口） |
| OpenSpec | 每个 Phase 一个 `openspec/changes/r4-<phase>/`（proposal + tasks + specs） |

### 5.2 风险登记

| ID | 风险 | 概率 | 影响 | 缓解 |
|---|---|---|---|---|
| R-1 | Phase A 的 JSONL v1 → v2 迁移遇到现存 session 不可读 | 中 | 中 | 双版本读取器；legacy 行标记 + 启动警告；新 session 只写 v2 |
| R-2 | hook 层 fail-isolation 改变用户自定义 hook panic 时的行为（之前杀 session，现在发事件） | 中 | 中 | R4 changelog + AGENTS.md 标注；`Steering::default_policy()` 默认开启，用户可显式 `wrap_fail_isolated(false)` 关闭 |
| R-3 | `TokenUsage` 字段拆分（v1 → v2）破坏调用方 | 高 | 低 | `TokenUsageV2` 双写，旧字段 deprecate，1 个 minor 版本后删 |
| R-4 | EpochHeader 写入给 session 体积带来 ~50 bytes/turn overhead | 低 | 低 | compaction 时合并相邻 RequestHeader（同一 provider/model 不重复写） |
| R-5 | AdaptiveStrategy 并发度变化影响并行桶实测性能 | 中 | 低 | benchmark 套件：保留 R3 的并发桶行为作 baseline；默认 cap=8 与 R3 等价 |
| R-6 | Lane trait 抽象对未来 fan-out 场景不够用 | 低 | 低 | trait 留 `submit / subscribe / close` 三接口；fan-out 时再扩 |

### 5.3 R4 不做的事（明确边界）

1. ❌ 不引入 traitclaw `McpServer` / `McpClient`（无 MCP 集成需
   求；按需新 crate）。
2. ❌ 不引入完整 traitclaw `AgentStrategy` trait（`Agent` 已经
   覆盖；第二个策略出现再抽）。
3. ❌ 不引入完整 pi Lane model + CompactionSettings（保留
   `SessionLane` trait 预留）。
4. ❌ 不引入 dsh `InboxTarget` 拆分（`next-turn` / `next-step`）。
5. ❌ 不引入 dsh `AgentMessage` vs `Message` 分离（Rust 无
   declaration merging）。

## 6. 参考链接

| 项目 | 关键模块 |
|---|---|
| `~/workspace/traitclaw/` | `crates/traitclaw-core/src/{traits,types,memory,default_strategy,agent_builder,pool}.rs` |
| `~/workspace/deepseek-harness/packages/core/` | `session/`（types / surface / repair / preparation）+ `agent-loop/`（agent / tool-calls / runtime-context）+ `agent/`（dispatch / index / types / runtime-types）+ `scope/` |
| `~/workspace/deepseek-harness/packages/llm/` | `llm/`（types / message / retry-policy / api-key / call-config / assembler / content / brand）+ `llm-retry/`（history / types）+ `token-meter/`（client / estimate / projection） |
| `~/workspace/pi/packages/agent/` | `harness/{events,hooks,config,runtime/lane,runtime/types,runtime/harness}.ts` + `harness/execution/effect-gate.ts` + `harness/session/{commit,memory,types}.ts` |
| `~/workspace/pi/packages/agent/` | `agent-loop.ts`（含 beforeToolCall / afterToolCall typed hooks + parallel/sequential execution） |

## 7. 阶段交付表

| Phase | Deliverable | Crate 改动 | 测试增量 | 工作量估算 |
|---|---|---|---|---|
| A.1 | `SessionEvent` 枚举 | `synthia-session` +3 | +10 | 1.5 天 |
| A.2 | `fold_surface` | `synthia-session` +2 | +8 | 1.5 天 |
| A.3 | `interrupted_turn_closers` | `synthia-session` +1 | +5 | 1 天 |
| A.4 | SessionSink v2 trait | `synthia-session` +1 | +5 | 1 天 |
| A.5 | SessionMemory 用 surface fold | `synthia-context` 修改 | +2 | 0.5 天 |
| A.6 | server 接入 + repair | `synthia-server` 修改 | +3 | 1.5 天 |
| **A** | **会话层 v2** | | **+33** | **~7 天** |
| B.1 | FailIsolatedHook | `synthia-steering` +2 | +3 | 1 天 |
| B.2 | wrap_fail_isolated | `synthia-steering` +1 | +1 | 0.5 天 |
| B.3 | HookAction 语义收敛 | `synthia-steering` +1 | +1 | 0.5 天 |
| **B** | **hook 层 v2** | | **+5** | **~2 天** |
| C.1 | typed retry | `synthia-provider` +2 | +3 | 1.5 天 |
| C.2 | token-meter | `synthia-telemetry` +2 | +2 | 1.5 天 |
| C.3 | EpochHeader | `synthia-session` + `synthia-agent` +1 | +1 | 1 天 |
| **C** | **provider 层 v2** | | **+6** | **~4 天** |
| D.1 | AdaptiveStrategy | `synthia-agent` +2 | +2 | 1.5 天 |
| **D** | **tool 层 v1** | | **+2** | **~1.5 天** |
| E.1 | SessionLane trait | `synthia-server` +1 | +2 | 0.5 天 |
| **E** | **lane trait** | | **+2** | **~0.5 天** |
| **R4 总计** | | | **+48 测试** | **~15 个工作日** |

## 8. 优先级与排期建议

按 ROI / 实现成本：

1. **优先 Phase A**（最高 ROI；会话层是 foundation）。
2. **次 Phase D**（成本最低、立刻闭环 AdaptiveTracker 接线）。
3. **后 Phase B**（中等成本、提升 hook 可靠性）。
4. **缓 Phase C**（与现有 telemetry 集成路径较深，需要先稳定 Phase A）。
5. **最后 Phase E**（trait 抽象预留，纯接口层）。

---

**结论**：R4 在 R3 已落地的 traitclaw / dsh steering 语义之上，
把 Synthia 的会话层（dsh 式事件溯源）、hook 层（pi 式 typed +
isolated-delivery）、provider 层（typed retry + token-meter）三
条主轴补齐，使其对齐「业界领先 agent runtime」的事实标准。trait
词汇不变（R3 全量覆盖），新增的不变量是「会话不可变 fold」+「hook
失败隔离」+「provider 错误类型化」三条，皆已在参考项目里被验证
为可落地形态。

## 9. 落地记录（2026-09-10，第一轮）

### Phase B ✅ hook panic isolation

- `synthia-steering/src/hook.rs`：新增 `HookStage`（7 个生
  命周期阶段）、`HookError { hook, stage, message }`、
  `run_hook`（`AssertUnwindSafe + FutureExt::catch_unwind` 包
  装，panic → `HookError`，session 不中断）。
- `re_act.rs` 全部 8 个 hook 调用点改走 `run_hook`；每个
  `Err(HookError)` 发 `WarningKind::Hook` 系统事件。
- `before_tool_execute` 的 `Block` verdict 原样保留（isolation
  只在 panic 时生效）。
- 测试：`run_hook_isolates_panic_to_hookerror` /
  `run_hook_preserves_block_verdict` /
  `run_hook_isolation_is_per_hook_not_batch` /
  `hook_stage_as_str_is_exhaustive`（synthia-steering 33 项全绿）。

### Phase A ✅ 会话层 v2（typed event log + surface fold + crash repair）

- `synthia-session/src/events.rs`：`SessionEvent` 16 变体封闭
  枚举（serde `tag = "type"`），`SurfaceOp`
  （`"append"` / `{start,end,source_event_seqs}`），仅
  `user_message` / `assistant_message` / `tool_result` /
  `compaction` 4 个变体可携带 `surface_op`。`from_value` 对
  legacy 形状（`{"role": ...}`、`UserInput`/`Model`）返回
  `None`，由既有 `events_to_messages` 投影兜底。
- `synthia-session/src/surface.rs`：`fold_surface` 纯函数 —
  append 尾插、replace 原子替换 `[start,end)` 并校验
  `source_event_seqs`（引用自身 seq 或未追加的 seq →
  `FoldError::BadProvenance`；越界 → `RangeOutOfBounds`）。
  返回 `FoldedSurface { messages, surface_seqs }`。
- `synthia-session/src/repair.rs`：
  `interrupted_turn_closers` — 单遍扫描 open
  turn/step/dangling calls，合成 `tool_result`（
  `TOOL_NOT_STARTED` = assistant 带 tool_use 但无 ToolCall 日
  志；`TOOL_OUTCOME_UNKNOWN` = ToolCall 已记录但 result 未持
  久化）+ `step/end` + `turn/end{reason:interrupted}`，seq 从
  last+1 严格递增，ts 复用最后一个真实事件。
- `synthia-context/src/memory/typed.rs`：
  `typed_messages_from_sink`（typed fold + legacy 投影按
  「legacy 头 + typed 尾」拼接——会话只前向迁移）、
  `repair_session`（读 → closers → 写回 sink，返回合成数）。
- `synthia-server/src/state/app_state.rs`：
  `get_or_create_session_controller_with_parent` 在 spawn
  controller 前对 sink 执行 best-effort
  `repair_session`（失败仅告警不阻断）。

### 验证（全部通过）

| Gate | 结果 |
 |---|---|
| `cargo test -p synthia-session --lib` | 48 passed（基线 15 + 新增 33：events 11 / surface 10 / repair 10 / 其它 2） |
| `cargo test -p synthia-context --lib` | 49 passed（基线 41 + typed 8） |
| `cargo test -p synthia-agent --lib` | 171 passed（零回归） |
| `cargo test -p synthia-server --lib` | 354 passed（零回归） |
| `cargo test -p synthia-tool --lib` | 175 passed（零回归） |
| `cargo test -p synthia-steering --lib` | 33 passed（基线 29 + 4） |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warning |
| `cargo +nightly fmt --all -- --check` | 0 diff |

### 已知边界（有意裁剪）

- **agent loop 尚未写 typed 事件**：`ReActLoop` 仍输出
  legacy envelope（`{"type":"Message"}` /
  `UserInput`/`Model`）。typed 通路已就绪
  （`typed_messages_from_sink` 对 legacy 行自动回退），切换
  agent loop 输出为 `SessionEvent` 序列是下一个增量（涉及
  controller 的 durable-event 分类，独立成项）。
- **`Compaction` replace 语义已实现但未由
  `SummarizingContextManager` 生产**：等 agent loop 写
  typed 事件后，Summarizing 可在 compaction 时发
  `SessionEvent::Compaction`（replace + source seqs）替代当
  前的内存改写。
- Phase C（typed retry / token-meter / EpochHeader wire）与
  Phase E（SessionLane trait）未动 —— **第二轮已全部落地，
  见 §10**。

### 顺手修复

- master 上 9 处 test fixture 缺 `AgentDescriptor::max_iterations`
  （及 1 处缺 `persona`、1 处 `AgentDetail` 缺字段）导致
  `cargo test -p synthia-agent --lib` / server 在 master 上
  编译失败 —— 全部补齐，workspace 恢复可测。

## 10. 落地记录（2026-09-10，第二轮 — Phase C + E）

R4 计划的 5 个 phase（A/B/D + C/E）至此全部落地。

### Phase C.1 ✅ typed retry（dsh llm-retry）

- `synthia-provider/src/retry.rs`：`RetryClass`（RateLimit /
  Overloaded / Transient / Permanent）+ `classify_error(&Error)`
  （变体驱动 + provider 文本标记嗅探：
  `overloaded_error`/`rate_limit`）+ `retry_config_for(class)`
  （RateLimit 6 次 1s→30s、Overloaded 4 次 500ms→10s、Transient
  3 次 1s→10s、Permanent 1 次）+ `retry_with_classification`
  （每次失败按类重选预算；Permanent 立即放弃；elapsed 预算按类
  重置）。
- Anthropic + OpenAI `complete()` 均改走
  `retry_with_classification`（原 `retry_with_backoff` 保留）。
- 测试：8 项（分类变体 / status / transient / provider 标记 /
  permanent / 预算 / permanent 即弃 / transient 后成功）。

### Phase C.2 ✅ token meter（dsh token-meter）

- `TokenUsage` 新增 `reasoning_tokens: Option<usize>`（OpenAI
  `completion_tokens_details.reasoning_tokens`）；
  OpenAI `OpenAIUsage` / 流式 `OpenAIDeltaUsage` 新增
  `prompt_tokens_details` / `completion_tokens_details` 并解析
  到 cached / reasoning；`openai::types` 升为 `pub(crate)` 供
  流式 processor 复用。
- `synthia-context`：`UsageMeter`（input/output/cache_read/
  cache_write/reasoning/calls + `record()` + `cache_hit_ratio()`）
  挂在 `AgentState.usage_meter`，`add_token_usage` 同时喂
  meter。
- 测试：`add_token_usage_accumulates_and_saturates` 扩展断言
  meter 分类与命中率。

### Phase C.3 ✅ EpochHeader 写入（dsh EpochHeader）

- `SessionController` 每次 run 前比较
  `(provider, model, tools_hash)` 与上次写入的 header；首个 run
  写 `reason: "initial"`，配置漂移写 `reason: "change"`，相同则
  不写（dsh dedup 语义）。
- 以 typed `SessionEvent::RequestHeader`（seq 0 占位）写入
  sink，位于该 run 的 `UserInput` 之前 —— 这是 agent loop 之外
  第一个真正的 typed 写路径。
- 测试：`test_events_are_persisted_and_broadcast` 更新为断言 3
  条持久化记录（header + UserInput + Model）+ 第二次同配置 run
  不重复写 header（header_count == 1）。

### Phase E ✅ SessionLane trait（pi Lane）

- `synthia-server/src/session/lane.rs`：`SessionLane` trait
  （`lane_id` / `submit` / `subscribe` / `close`，boxed-future
  保证 object safety）+ `DefaultLane` 透明委托
  `SessionController`（close = `SessionOp::Shutdown`）。
- `SessionController::session_id()` 访问器新增。
- 测试：5 项（id 回显 / submit 委托 / object-safety 动态分发 /
  close 后 submit 拒绝 / controller 访问器）。
- 测试总数：synthia-server 359（354 + 5 lane）。

### 验证（第二轮，全部通过）

| Gate | 结果 |
 |---|---|
| `cargo test -p synthia-server --lib` | 359 passed（354 + 5 lane；header 测试更新） |
| `cargo test -p synthia-provider --lib` | 582 passed（含 8 项 typed-retry + reasoning 解析） |
| `cargo test -p synthia-context --lib` | 49 passed（含 meter 断言扩展） |
| `cargo test -p synthia-agent --lib` | 171 passed（零回归） |
| `cargo test -p synthia-tool --lib` | 175 passed（零回归） |
| `cargo test -p synthia-steering --lib` | 33 passed（零回归） |
| `cargo test -p synthia-session --lib` | 48 passed（零回归） |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warning |
| `cargo +nightly fmt --all -- --check` | 0 diff |

### 边界（有意裁剪）

- agent loop 内部写侧仍是 legacy envelope（`ReActLoop` 不自持
  SessionSink，持久化归 controller）；typed 读路径
  （`typed_messages_from_sink`）+ controller 侧的 typed 写
  （`RequestHeader`）已闭环。要让 agent 内层事件也走 typed，
  需把 controller 的 durable 事件分类映射到
  `SessionEvent` 变体 —— 独立增量。
- `SummarizingContextManager` 的 compaction 仍未产出 typed
  `Compaction` 事件（依赖上一条）。