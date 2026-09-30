# Synthia × TraitClaw 差距与集成分析

> 参考 `~/workspace/traitclaw/` 的 trait 定义，提炼对当前 Synthia
> (8-crate Rust workspace + Vite/React 前端) 可直接借鉴 / 落地的 trait，
> 完善整个 AI Agent 的可组合性、可观测性与安全性。

> **落地记录（2026-09-06，第二轮）**：Phase 1 全量落地 ——
> 新增 `crates/synthia-steering`（`Guard` / `AgentHook` / `Hint` /
> `Tracker` / `OutputTransformer` / `Action` / `Steering` bundle），
> `ReActLoop` 五个 interception seam 接线（hooks → guards →
> tracker → hints → transformer），server 侧
> `Steering::default_policy` 全局注入。同时修正了 traitclaw 自身的
> 四个接线缺陷（on_tool_call 死代码、guard 跨 run 状态泄漏、
> Sanitize 丢弃、injection_point/priority 被忽略）与 severity 未消费。
> `ExecutionStrategy` 以 dsh rolling-pool 语义落地为
> tracker 推荐并发 + `Semaphore` 封顶（保留按工具 ExecutionMode
> 分桶 + LLM 原序提交），不再引入整批 trait。shell 工具同步对齐
> dsh（cwd、进程组击杀 SIGTERM→grace→SIGKILL、timeout 上限、
> 非零退出=数据 + 尾部标记、NO_COLOR/TERM 环境加固）。
> 详见第 8 节。
## 1. TraitClaw 的 8 大核心 trait 一览

| Trait | 关注点 | 阶段 | 同步 vs 异步 |
|---|---|---|---|
| `Provider` | LLM API 抽象 | 运行期 | async |
| `Tool` / `ErasedTool` | 强类型 + JSON Schema + 类型擦除 | 运行期 | async |
| `Memory` | 3 层（conversation / working / long-term） | 运行期 | async |
| `Guard` | 行动前硬拦截（deny / sanitize / allow） | 运行期 | **sync** |
| `Hint` | 上下文引导注入 | 每轮迭代 | sync |
| `Tracker` | 运行时状态观察 | 每轮迭代 | sync |
| `ContextManager` | 上下文窗口规划（async，LLM 摘要可用） | 每次 LLM 前 | async |
| `OutputTransformer` | 工具输出后处理（带 tool_name + state） | 每次工具后 | async |
| `ExecutionStrategy` | 工具批并行/串行/自适应 | 每批工具 | async |
| `AgentStrategy` | 整个推理循环的策略（ReAct / CoT / MCTS） | 一次会话 | async |
| `AgentHook` | 生命周期观测 + 拦截 | 关键节点 | async |
| `ToolRegistry` | 动态注册/启用/分组 | 运行期 | sync (写内部用 `RwLock`) |

附带策略型 trait：ReActStrategy、ChainOfThoughtStrategy、MCTSStrategy。

## 2. Synthia 现有 trait 状态

| Synthia 类型 | 位置 | 已覆盖 TraitClaw 中的 |
|---|---|---|
| `ModelProvider` | `synthia-provider/src/traits.rs` | `Provider` |
| `Tool`（带 `stream` / `call` / `truncate`）| `synthia-tool/src/traits.rs` | `Tool` + `ErasedTool`（含 `stream`） |
| `ToolRegistry` | `synthia-tool/src/registry.rs` | `ToolRegistry`（更强：含 `RegistrationToken` / scope / provenance） |
| `Agent`（async + Stream 输出）| `synthia-agent/src/agent/mod.rs` | `AgentStrategy`（已工业对齐） |
| `ReActAgent` | `synthia-agent/src/agent/re_act.rs` | 默认 `AgentStrategy` |
| `AgentRegistry` | `synthia-agent/src/agent/registry.rs` | 自有 |
| `SessionSink` | `synthia-session/src/sink.rs` | 等价于 `Memory::messages + append + close` |
| `PromptContext::assemble` | `synthia-agent/src/prompt/mod.rs` | 独有的 XML 段组装 |

## 3. 缺口：Synthia 缺少 TraitClaw 中的关键 trait

按"补齐后收益 / 落地难度"排序：

### P0 — 高收益 / 低成本

1. **`Guard`** — 行动前硬拦截
   - 当前 Synthia 的 `ShellTool` 内置 `DEFAULT_DENY_PATTERNS`，但只是工具内
     的正则黑名单。缺少 **工具无关** 的拦截层（HTTP 出口、敏感路径写入、
     委派目标校验、Token 配额等）。
   - 接口形态：`trait Guard: Send + Sync { fn name() -> &str; fn check(&Action) -> GuardResult; }`
   - 同步（μs 级）；由 `ToolRegistry::dispatch` 在每个 `ToolCall` 前调用。
2. **`AgentHook`** — async 生命周期拦截
   - 当前 Synthia 的可观测性散落在 `tracing` span + `metrics` 中间件，**没
     有统一的 trait 层**。补上后能：
     - 在 `synthia_server` 里挂 `MetricsHook` / `AuditLogHook` / `TracingHook`
       而不污染 agent 代码。
     - 通过 `HookAction::Block(reason)` 在「拒绝高风险工具」这种边界用
       例里替代部分 Guard 职责。
   - 方法：`on_agent_start / on_provider_start / on_provider_end /
     before_tool_execute / after_tool_execute / on_stream_chunk /
     on_error / on_agent_end`，全部 async + 默认 no-op。
3. **`Hint`** — 模型引导注入
   - 当前没有"实时提醒"机制。当检测到模型连续重复同类型工具调用、即将
     触发 token 超限、或忽略已有工具输出时，可注入一条 `[reminder]`
     消息。
   - 接口形态：`trait Hint: Send + Sync { fn should_trigger(&AgentState) -> bool; fn generate() -> HintMessage; fn injection_point() -> InjectionPoint; }`
   - 4 个 `InjectionPoint`：SystemPrompt / BeforeNextLlmCall / RecencyZone /
     AppendToToolResult{ tool_call_id }。
4. **`Tracker`** — 运行时状态观察
   - 给 `Hint` 和自适应 `ExecutionStrategy` 提供信号源：迭代次数、token
     利用率、最近一次输出是否被截断、是否在团队任务中等。
   - 接口形态：`trait Tracker { on_iteration / on_tool_call / on_llm_response / recommended_concurrency() }`。

### P1 — 中收益 / 中成本
5. **`ContextManager`** — 异步上下文窗口规划
   - **状态（2026-09-06）**：已落地为独立 crate `crates/synthia-context`：
     `ContextManager` trait + `TruncatingContextManager`（默认）、
     `SummarizingContextManager`（pi-context-prune 移植）、
     `DagContextManager`（pi-lcm 移植）；`ReActAgent::with_context_manager`
     接线 + `WarningKind::ContextCompaction` 告警。
   - 当前 Synthia 的截断只在 `truncate::bound_output`（工具输出级），
     **没有会话级**的"消息列表压缩/淘汰"策略。补上后：
     - 默认 `TruncateContextManager`（drop oldest non-system，保持最近 N
       轮）。
     - 预留 `LlmSummarizeContextManager`（调用 provider 摘要旧轮次）作为
       feature flag。
6. **`OutputTransformer`** — 工具输出后处理
   - 与 `truncate` 重叠但更通用：按 tool_name + AgentState 决策（比如
     `web_fetch` 长结果在低预算时降采样；`shell` 错误自动去 ANSI）。
7. **`ExecutionStrategy`** — 工具批量调度
   - Synthia 现有 `ExecutionMode::{Parallel, Sequential}` 是工具级静态属
     性，缺少**批次级**决策（"当前批并行跑还是串行？"、"tracker 推荐
     concurrency = 2，启用 AdaptiveStrategy"）。补上后能直接对接
     `tokio::join!` 的批并行路径。

### P2 — 长期 / 重构
8. **`Memory` 3 层抽象** — `Memory` 当前被 `SessionSink` 单层替代。
   - **状态（2026-09-06）**：已落地在 `synthia-context::memory`：
     `Memory` trait（conversation / working / long-term 三层 + 生命周期默认实现）、
     `InMemoryMemory`、`SessionMemory`（conversation 层投影自 SessionSink JSONL，
     `with_max_events` 有界 rehydration；working / long-term 进程内）、
     `events_to_messages` 共享投影（服务端 controller 原私有 `events_to_history`
     已删除改为复用）。`SessionSink` 保持 inert leaf 不变（context → session 单向）。
9. **`AgentStrategy` 显式化** — 当前 `Agent` trait 已经覆盖了，但
   `ReActAgent` 是 hard-coded。提取 `ReActStrategy` 后能让
   `ChainOfThoughtStrategy`、`PlanExecuteStrategy` 共存。属于"已经隐式
   有了"，只需要把 `Agent::run` 拆成 `AgentStrategy::execute(runtime,
   input, session_id)`。

## 4. 推荐落地计划（针对当前 Synthia）

### Phase 1 — 补 Guard / Hook / Hint / Tracker 四个 sync trait + 一个 Action 类型
**新增 crate：`synthia-steering`**

```
crates/synthia-steering/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── action.rs        // Action enum: ToolCall / ShellCommand / FileWrite / HttpRequest / AgentDelegation / RawOutput
    ├── guard.rs         // Guard trait + GuardResult + NoopGuard + Severity
    ├── hint.rs          // Hint trait + InjectionPoint + HintPriority + HintMessage + NoopHint
    ├── tracker.rs       // Tracker trait + AgentState (synthia-provider 类型映射) + NoopTracker
    ├── hook.rs          // AgentHook trait (async) + HookAction + LoggingHook
    └── noop.rs          // 一个 Steering::noop() 聚合
```

**集成点（最小侵入）：**
- `synthia-agent`：把 `Vec<Arc<dyn Guard>>` + `Vec<Arc<dyn AgentHook>>`
  + `Arc<dyn Tracker>` 字段加到 `ReActAgent`；在
  `ReActLoop::execute_tool` 入口前调用 Guard，结束调用 Hook。
- `synthia-tool::registry`：`dispatch` 暴露的回调点同时触发 Guard 拒绝
  决策与 Hook 拦截。
- `synthia-server`：注册 `LoggingHook` + 简易 `MetricsHook`（计
  数 on_provider_end 的 token、tool 调用次数）。
- 不引入 `Memory` / `ContextManager` / `OutputTransformer` / `ExecutionStrategy`
  到 P1 之前，避免一次性大重构。

### Phase 2 — 把 `ContextManager` / `OutputTransformer` / `ExecutionStrategy` 接入 `ReActLoop`
- `ContextManager::prepare(&mut messages, ctx_window, &mut AgentState)`
  在 `sample_once` 前调用。
- `OutputTransformer::transform(output, tool_name, &state)` 在
  `commit_tool_result` 时包一层（与 `truncate::bound_output` 串联：先
  transform 再 truncate）。
- `ExecutionStrategy::execute_batch(pending)` 替换 `ReActLoop` 内
  `tokio::join!` 串接，AdaptiveStrategy 可读 Tracker 的 concurrency。

### Phase 3 — `AgentStrategy` 显式化
- 抽 `trait AgentStrategy: Send + Sync` + `AgentRuntime { provider,
  tools, registry, guards, hooks, ... }`。
- 把 `ReActAgent::run` 替换为 `self.strategy.execute(runtime, input,
  session_id).await`。
- 增加 `CotStrategy` / `PlanExecuteStrategy` 作 v0.2 的可选策略。

## 5. 不引入的 trait 及理由

| Trait | 不引入理由 |
|---|---|
| `McpServer` / `McpClient` | Synthia 当前没有 MCP 集成；如未来要加，单独 crate。 |
| `Team` / Router | Synthia 的等效委派层已落地（2026-09-06）：`task` 工具接缝 —— `ReActAgent::with_peer_registry` 给模型暴露 `task(agent, prompt)`，循环内拦截 → `AgentRegistry::resolve_sync` 解析 peer → 子 agent 运行，事件以 `AgentEvent::Agent(AgentMeta)` 包裹转发（`parent_depth` 递增，`child_session_id` = 每次委派铸 ULID，供前端/回放分组并行子级），`MAX_SUBAGENT_DEPTH=3` 防递归，子最终文本作为 tool result 提交；server 工厂 + `build_react_agent` + 默认 agent 均注入共享 registry。多 agent fan-out / sequential / leader 拓扑由调用方在事件流上层组合（与 traitclaw `Router` 的 `RoutingDecision` 对应语义保持 server 侧策略，不进 agent 内核）。
   **前端映照（synthia-web，同日）**：live SSE 不再压平 `Agent` 帧 —— 子内容渲染为独立 `SUB-AGENT · depth N` 气泡（按 child trace id 分组，并行子级互不混淆），子级自己的 `SessionEnded` 被抑制、绝不提前终结父轮；`System(Warning)`（含 `context_compaction`）渲染为转录内的提示条而非状态翻转；会话详情回放（`reconstructMessagesFromSession`）同样把持久化的递归 `Agent` 行还原为子气泡。单测 + Playwright UI 全绿。

## 6. 关键决策点（已裁决，2026-09-06）

1. `Action` 放在 `synthia-steering` 内（不给 `synthia-core` 加重量）。✅
2. `HookAction::Block(reason)` 直接把 reason 写进 tool result（语义与
   工具错误一致）；guard 拒绝额外发 `WarningKind::Guard` 系统事件
   （前端 kind 透传渲染为转录内提示条，已核验安全）。✅
3. `AgentState` 复用 `synthia-context::AgentState` 并扩展
   `total_tokens` / `tool_call_count` / `recent_tool_fingerprints`
   （单一运行态真相源；guard 无内部可变状态，天然免跨 run 泄漏）。✅
4. `Guard` 保持 sync（μs 级策略检查）；远端策略点未来升 `async fn`
   即可。✅

## 7. 验收标准（Phase 1，全部达成 ✅）

- [x] 新增 `crates/synthia-steering` crate，`cargo build/test
      -p synthia-steering` 通过；workspace clippy 0 警告。
- [x] `ReActAgent::with_steering` 安装 bundle；默认
      `Steering::noop()`，行为与改造前完全一致（164 项既有
      agent 测试零回归）。
- [x] `synthia-server` 启动即注入 `Steering::default_policy`
      （默认 agent、`build_react_agent`、session factory 三处），
      含 `LoggingHook`。
- [x] 单元测试：NoopGuard 永远 Allow；IterationReminder 按倍数触发；
      guard 拒绝 → error result + `WarningKind::Guard` 事件 +
      session 正常 Completed；hook Block → error result；Sanitize
      动作被采纳（重写到 alt 工具真实执行）；hint 注入抵达下一次
      LLM 请求尾部；transformer 改写提交结果；guard panic
      fail-closed。
- [x] 回归：`cargo test -p synthia-agent`（169）、
      `-p synthia-tool`（175）、`-p synthia-server`（354）全绿。

## 8. 第二轮落地明细（2026-09-06）

| 层 | 内容 | 位置 |
|---|---|---|
| trait 层 | `Action`（6 变体 + from/apply_to/fingerprint/summary + 键序规范化指纹）、`Guard`/`GuardResult`/`GuardSeverity` + `run_guards`（catch_unwind fail-closed、sanitize 折叠）、`Hint`/`InjectionPoint`(4)/`HintPriority`、`Tracker`（观测 + 推荐并发）、`AgentHook`(8 钩子) + `HookAction`、`OutputTransformer`、`Steering` bundle（noop/default_policy/builder） | `crates/synthia-steering/src/*.rs` |
| 内置守卫 | LoopDetection（连续同指纹）、ToolBudget（Critical + 收尾话术）、ShellDeny（灾难级清单）、WorkspaceBoundary（词法归一化）、PromptInjection（高精度 regex，不扫文件内容） | `synthia-steering/src/guards.rs` |
| 内置提示 | ContextBudgetHint（利用率阈值）、IterationReminderHint、TruncationHint | `synthia-steering/src/hint.rs` |
| 运行态 | `AgentState` 扩展 3 个 steering 字段 + `record_tool_call`/`identical_tail_run`/`add_token_usage` | `synthia-context/src/context_manager.rs` |
| 循环接线 | 5 个 seam：hooks(before/after/provider/start/end/on_error) → guards（hook 先 veto、guard 后硬策略不可覆盖；拒绝=errors-as-results + `WarningKind::Guard` 事件；sanitize 采纳为 effective call）→ tracker（迭代/响应/工具边界 + `Semaphore` 并发封顶）→ hints（4 个注入点全实现 + priority 门控）→ transformer（文本投影改写，错误输出原样透传） | `synthia-agent/src/agent/re_act.rs` |
| server | `AppState::steering`（default_policy）注入默认 agent / `build_react_agent` / `RunDependencies`→`AgentRunConfig`→factory | `synthia-server/src/state/app_state.rs`、`session/controller.rs`、`routes/agents.rs` |
| shell 加固 | workspace cwd、`process_group(0)` + SIGTERM→3s grace→SIGKILL 整树击杀、timeout 默认 120s / 上限 600s（钳制）、非零退出=正常结果 + `[exit code: N]` 尾标记（超时 `[timed out after Ns]`、信号 `[killed by signal: N]`）、NO_COLOR/TERM=dumb/PAGER=cat/GIT_PAGER=cat | `synthia-tool/src/builtin/shell.rs` |
| 配置化迭代上限 | 拆掉硬编码 `const MAX_ITERATIONS = 25`；`ReActAgent.max_iterations` 字段 + `with_max_iterations` (clamp `[1, 4096]`);`AgentRunConfig`/`RunDependencies`/`AppState::default_max_iterations` 一路接到 factory；server config `agents.<name>.max_steps` (u32 → usize) 默认；`POST /agents` body 接受 `max_iterations` 覆盖 | `synthia-agent/src/agent/re_act.rs`、`config.rs`、`synthia-server/src/{state/app_state.rs, session/controller.rs, routes/agents.rs}` |
单工具的 ExecutionMode，语义弱于现有实现）。按 dsh 的 rolling-pool
语义落地：保留 synthia 的 Parallel/Sequential 分桶 + LLM 原序提交，
并行桶用 `Tracker::recommended_concurrency` 驱动的 `Semaphore` 封顶
（`AdaptiveTracker` 按上下文压力 8→2→1 降档）。

**已知后续项**（未做，属有意裁剪）：
- `Memory` 三层已由 `synthia-context::memory` 覆盖；traitclaw 的
  `CompressedMemory` 装饰器（LLM 摘要栈叠）已有等价物
  `SummarizingContextManager`。
- `AgentStrategy` 显式化（Phase 3）：`Agent` trait 已覆盖；等第二个
  策略（CoT/Plan-Execute）真正出现时再抽。
- traitclaw `Team`/`Router`/`VerificationChain`：委派已由 `task`
  工具 + peer registry 覆盖；生成-验证-重试链待真实用例。
- dsh 的 session envelope 版本化（`ignorable` 协议）与 durable
  retry 计数恢复：独立大项，见
  `synthia-dsh-alignment-review` skill 的 session 对照表。
