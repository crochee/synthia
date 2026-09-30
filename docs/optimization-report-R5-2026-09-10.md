# Synthia R5 — Multi-Expert Gap Analysis (traitclaw × dsh × pi)

> **基线**：R3（traitclaw 五件套） + R4（dsh `surface/fold/repair` +
> pi `run_hook fail-isolation` + `RetryClass` typed retry + token meter +
> `EpochHeader` + `SessionLane` trait）全部落地，R4 报告
> `docs/optimization-report-R4-2026-09-10.md` 列出 +48 测试增量。
>
> **R5 目标**：跨 6 个轴做一次系统性 gap 分析——单点特性 R4 已
> 收齐；R5 关注的是**系统级不变量**（贯穿多 crate 的契约、状态机的
> 完整性、可观测性的可归因性）。每条发现都对照三个参考项目的对
> 应物，给出落点 / 风险 / ROI。
>
> **范围声明**：所有发现基于 `read` 实测源码（`crates/*/src/**`） +
> R4 报告 + traitclaw / dsh / pi 三方对应模块的对照阅读。
> `[INFER]` 标注的派生结论不代表已经被官方背书。

---

## 0. 阅读路径

1. §1 — R4 后仍未覆盖的"系统级"缺口在哪六个轴上。
2. §2 — 每个轴的 gap statement（2-3 段 + 代码引用）。
3. §3 — 跨轴的 10 项 R5 采纳候选（按 ROI 排名）。
4. §4 — 边界 / 风险登记。
5. §5 — 参考链接。

---

## 1. 系统级 gap 的分布

R4 把"单 trait / 单事件 / 单错误类型"的硬骨头啃完了。R5 留下来
的是「贯穿多个 crate 的契约完整性 + 操作期可归因性」——这是单
PR 难以解决、必须按轴打磨的工程问题。

| 轴 | 现状一句话 | R5 关心什么 |
|---|---|---|
| 1. agent lifecycle / ReAct 完整性 | 5-step loop + hooks/guards/hints/tracker/transformer 接线 ✅（R3） | 跨 iteration / 跨 sub-agent 的状态不变量、Iteration / Step / Turn 边界事件、abort 路径不变量 |
| 2. tool 执行完整性 / sandboxing / 取消 | shell 工具 dsh 加固 ✅、panic isolation ✅ | 并发取消传播、跨工具写竞争（mutation queue）、barrier pool 中 commit ordering |
| 3. provider 抽象 / 多模型 | typed retry ✅、cache policy ✅、Reasoning/cache_read token ✅ | StreamChunk 的 typed assembler、provider 取消的统一语义、缓存前缀验证 |
| 4. observability / tracing / metrics | OTel + 文件日志 + HTTP RED ✅、Usage event ✅ | span / metric 归因到 agent/tool/turn、token-meter 与 session 持久化联动、failure 端到端 trace |
| 5. session / 持久化 / replay / repair | typed event + surface fold + repair ✅ | legacy envelope 的 typed 迁移、Compaction replace 上游、SessionPreparation / 子 lineage |
| 6. developer experience / API / docs | OpenAPI ✅、4 builtin skills ✅ | typed event schema 的可读暴露、`/agents/:id/events` 流式 dump、Per-tool replay endpoint、CLI 对 server state 的 introspection |

---

## 2. 按轴的 gap statement

### 轴 1 · Agent lifecycle / ReAct loop integrity

**当前形态**：ReActLoop 5-step (`prepare → sample → commit → execute
→ finalize`) + 4 个 `Steering` seam + `task` 子委派 + R4 typed retry。
Hook 失败通过 `run_hook` 隔离；guard 拒绝 errors-as-results；
output transformer 改写 commit 文本。代码层在
`crates/synthia-agent/src/agent/re_act.rs`（5763 行）+ `delegation.rs`
+ `synthia-steering/{hook,guard,hint,tracker,output_transformer}.rs`。

**gap 1.1 — 缺 Iteration / Step / Turn 边界事件**

dsh 的 `known-event-types.ts` 列出 64 个 wire-level event type，其中
`iteration/start|end`、`step/start|end`、`turn/start|end` 是结构
性事件。Synthia 当前 `SessionEvent`（`crates/synthia-session/src/events.rs:117-294`）
的 16 variant 里**没有** Step / Turn / Iteration 边界——只有
`RequestHeader` / `UserMessage` / `AssistantMessage` /
`ToolResult` / `Compaction` 等"内容事件"。后果：

- `interrupted_turn_closers`（`repair.rs:74`）的"scan open
  turn / open step"逻辑只在合成路径上才存在，replay 工具无法
  从 JSONL 知道"这是 turn 7 的第 3 步"。
- 跨 agent 委派的边界没有持久化：parent_session_id /
  child_session_id 在内存里活着，server 重启后靠
  `AgentEvent::Agent(AgentMeta)`（`event_enum.rs:42`）重建 lineage，
  但 JSONL 里只有一次扁平嵌套。
- frontend 给一个 multi-iteration 画"几条消息"完全靠启发式——
  没有 iteration_idx 字段可读。

**gap 1.2 — Abort / 取消在 hot path 上分散**

`ReActLoop::drive` 在 4 处手工 `self.cancelled(...)`：
`re_act.rs:658`（LLM 前）、`re_act.rs:721`（execute 前）、
`re_act.rs:1314`（sequential bucket）、`re_act.rs:1698`（dispatch
mid-stream）。R4 报告 §3.2 G-5 把 `pi` `EffectGate` 标为"可选落地"——
但 R5 来看这是 ReAct 完整性的硬不变量：4 处手检意味着每次新增一个
await 点（比如未来 swap in 一个 LLM-summarizing context manager）
就要手动复制 cancel 检查；漏一处就 panic propagate 到 session 而不
是干净退出。pi `effect-gate.ts` 的 `gate.admit(invoke)` 把这层抽
象成 `AbortRequested` 抛错机制；Synthia 用 `CancellationToken` 是
同样的语义但每条 hot path 都手检。

**gap 1.3 — 委派的 parent lineage 缺 durable 标记**

`delegation::run_subagent`（`delegation.rs:130-192`）生成 `AgentMeta`
携带 `parent_session_id` / `child_depth`，但只在 `AgentEvent::Agent`
包装层；session JSONL 里看不到 `subagent_enter` / `subagent_exit`
的独立事件——R4 报告 §9 把这个列入已知边界："agent loop 内部写
侧仍是 legacy envelope"。所以 server restart 之后无法回放
"这个 turn 的第 2 步 fork 了 sub-agent child_xyz"。

---

### 轴 2 · Tool execution integrity / sandboxing / cancellation

**当前形态**：`ToolRegistry::run_stream`（`registry.rs:524-606`）做
per-call semaphore（默认 5 并发）+ `consume_tool_stream_into`
（`registry.rs:790-852`）的 panic isolation + `consume_tool_stream_into`
合成的 "no Result / panicked" 错误。`ShellTool` 已对齐 dsh 进程组
击杀 + timeout + 环境加固（`shell.rs:1-86`）。`ReActLoop::execute_tools`
（`re_act.rs:1224-1419`）按 `ExecutionMode` 分桶，tracker 推荐并发
真正接到 `Semaphore::new(concurrency)`（R4 G-4 落地）。

**gap 2.1 — 缺 mutation queue（pi `file-mutation-queue`）**

`pi/packages/agent/src/harness/tools/file-mutation-queue.ts` 是 60 行
的小模块：它把"同一 canonical path 的写操作"排队（per-env Promise
chain），防止两个并行 write 工具在同一文件上竞争。Synthia 的
`WriteTool`（`builtin/write.rs`）是 `ExecutionMode::Sequential` 的默
认行为，**整个 registry 也只在 tool-level 静态分桶**——它不识别
"两个不相关的 write 工具同时写同一个文件"这种动态冲突。
`test_run_with_context_concurrent`（`registry.rs:1327-1378`）测两
个 `FastTool` 并发，但 FastTool 不写文件；如果换成两个
`WriteTool`（同名 / 不同名）写到同一路径，在并发桶里就会竞争。

**gap 2.2 — `run_stream` 取消粒度 = drop-the-stream**

`registry.rs:524` 的 `run_stream` 在注释里写："Cancellation is
implicit: dropping the returned stream aborts all in-flight tasks."
但 `execute_tools`（`re_act.rs:1286-1308`）的并行桶没有 drop
stream——它 `join_all` 等待所有 future 完成；sequential bucket
（`re_act.rs:1313`）会 `is_cancelled()` 跳出循环，但已经 dispatched
的那个 tool 会跑到自然结束。`shell.rs:280-363` 的 `execute` 内部
会响应 SIGTERM（dsh parity），但 read / write / todo / web tool
**没有 process-group kill 等价物**——一个 30s 的 web_fetch 即使用
户 cancel 了，也要等到 HTTP timeout。R4 G-3 把
"tool-level cancellation token" 标为低 ROI 候选；R5 来看，sub-agent
被 cancel 时它的子工具应该立即被打断，而不是等到 network 收尾。

**gap 2.3 — dispatch panic isolation 缺 metric**

`consume_tool_stream_into`（`registry.rs:804-834`）在 panic 时
`record_tool_outcome(&span, &out)` 设置 `exception.type` /
`otel.status_code`，但**不更新 Prometheus**。`metrics.rs:30-54`
的 `HTTP_REQUESTS_TOTAL` / `HTTP_REQUESTS_DURATION_SECONDS` 只覆盖
HTTP RED；tool panic 次数、tool duration histogram、tool-result
truncation 频次都没有埋点。结果：operationally 想知道"是不是某
个 tool 经常 panic / 经常触发 truncation"必须 grep JSONL。

---

### 轴 3 · Provider abstraction / multi-model

**当前形态**：`ModelProvider` trait（`traits.rs:53-107`）4 个方法
（`initialize` / `name` / `model_config` / `complete` /
`complete_with_stream` / `embed`）+ Anthropic + OpenAI + cache_policy
+ `RetryClass` typed retry（`retry.rs:186-332`）+ Anthropic /
OpenAI 都实现了 5 秒 cancel grace（`anthropic/traits_impl.rs:188-213`
+ `openai/traits_impl.rs:206-220`）。`TokenUsage` 已扩展
`reasoning_tokens`（R4 C.2 落地），但仍只有 6 个字段。

**gap 3.1 — 缺 `BlockAssembler`（dsh `assembler.ts`）**

dsh 的 `BlockAssembler`（`packages/llm/llm/src/assembler.ts`）是一
个**容错的增量装配器**：它把 provider 的 `StreamChunk` 序列装
成完整的 `Message`，对 delta-only 协议（没有 block-start/end）也
能 work；malformed stream 时对已 closed 的 block 忽略掉 stragglers。
Synthia 当前 `sample_once`（`re_act.rs:989+`）走的是
`ChunkStateInner`（`re_act.rs:1881+`，saw_streamed_text / tool_use_id
去重），逻辑揉在 ReAct loop 内 200+ 行；它对 partial block 的处理
是 ad-hoc 的（注释 `re_act.rs:1895-1903` 解释 `IsDone` 重复覆盖的
fallback）。把这段抽成 `BlockAssembler`：

- 让 `ModelProvider` 只需要 expose `Stream<Result<StreamChunk, Error>>`
  而不是把"在 ReActLoop 里逐 chunk 装"的内幕暴露给 provider
  实现者。
- 让测试可以单测 assembler（partial / straggler / malformed）而
  不必开一个完整的 ReActAgent。

**gap 3.2 — Token 用量字段语义仍 flat**

R4 把 `reasoning_tokens` 加到 `TokenUsage`；dsh 的 `Usage` /
`UsageRow` 还把 cache_creation / cache_read / reasoning 单独计费
（dsh `token-meter/src/types.ts` 的 `Usage` + `UsageRow` 分层）。
Synthia 当前 `TokenUsage`（R4 后 6 字段）所有字段都是平铺的
`Option<usize>`，没有按 provider 区分语义。后果：

- 跨 session 归因"昨天 100k 的 cache_read"无法分摊到具体 agent
  / tool / turn——`UsageMeter`（`synthia-context/src/context_manager.rs`
  R4 新增）只是按 6 类相加。
- 限额 / quota（"每个 user 每天 1M cache_read tokens"）目前没有
  落点。

**gap 3.3 — 缓存前缀验证只到 Arc-ptr equality**

`CachePolicyApplier`（`cache_policy.rs:139-203`）用 `Arc::ptr_eq`
判断 system / messages 是否变化——这是 opencode 的"reference
equality" 语义，省去了重算 cache marks 的开销。但 Synthia 在
ReActLoop 里没有保证 messages vec 的 Arc 复用——`apply_context`
（`re_act.rs:879-900`）会调 `ContextManager::prepare(messages, state)`
裁剪历史，**裁剪后是新的 `Vec<Message>`**（不
是 `Arc<Vec<Message>>`），所以 ptr_eq 永远失败，每次都重新打
mark——cache 命中率为 0。R5 来看这是 cache policy 与
`ContextManager` 的契约空缺：要么 `ContextManager` 输出
`Arc<Vec<Message>>` 让 ptr_eq 生效，要么 `CachePolicyApplier` 升
级到内容哈希。

---

### 轴 4 · Observability / tracing / metrics

**当前形态**：`synthia-telemetry` 提供 OTel OTLP gRPC/HTTP + 控制
台 fallback（`tracer.rs:158-243`）+ 文件日志层（`tracer.rs:287-315`）
+ `W3C TraceContext` 透传（`propagation.rs`）+ Prometheus RED 端点
（`metrics.rs`）。ReAct loop 有 `react_loop` span + `tool.execute`
span（registry.rs:572-578）+ `react_loop` 内的迭代计数 span fields
（`re_act.rs:600-608`）。`AgentEvent` 携带 `iteration: Option<u32>`
（`WarningKind::Hook` 等），但**没有 span attributes 自动 attach**。

**gap 4.1 — Trace 没有 agent / turn / step 维度**

OTel span 的常见 attribute 是 `agent.name` / `agent.id` /
`session.id` / `turn.id`。Synthia 的 `react_loop` span
（`re_act.rs:600-608`）只有 `agent = %self.descriptor.name` +
`max_iterations`——没有 `session.id` / `turn.id`。结果是：

- 一段 trace 里多个并发 session 互相区分只能靠 grep 文本。
- provider 层（`anthropic/traits_impl.rs`）的 streaming span
  没有 inherit 上层的 `session.id`——OTLP 后端的"按 session 聚合"
  视图需要手工 trace stitching。

**gap 4.2 — 缺 token-meter 与 session 持久化的桥**

dsh `token-meter` 把每次 LLM 调用的 usage 写到 usage_events 表
（durable），session replay 能查到"这一段 trace 一共多少
tokens / cache hit ratio"。Synthia 的 `UsageMeter`（R4 C.2）在
进程内挂在 `AgentState` 上；session JSONL 里没有 usage 持久化
事件——session detail 的"cost / token"视图只能从当前 in-memory
state 读，server restart 后全失。`SessionEvent` enum
（`events.rs:117+`）的 16 variant 里 `UsageRecord` 是 R4 报告
规划的字段，但**没有实现写入路径**——只在 R4 报告 §4.2 A.1 的
schema 草图里出现。

**gap 4.3 — 失败端到端 trace 不完整**

当 agent run 因 panic / 致命 stream error 中断时，OTel span 树
会有 `react_loop` 父 span + 失败的子 span，但没有 `agent.run`
总结 span 把"failure_reason = ..." 写到 attributes。pi 的
`startHarnessSpan('hook', { name })` 把 hook 名字写到 span field
（`pi/agent/src/harness/telemetry.ts`）；Synthia 的 hook span 是
R4 报告 G-2 标注的"自动开 tracing span"，但**实际代码里
没有落地**——`run_hook`（`hook.rs:149-175`）只包 `catch_unwind`，
没有开 `tracing::info_span!("hook", name = ...)`。

---

### 轴 5 · Session / persistence / replay / repair

**当前形态**：`SessionSink` 5-method trait（`sink.rs:76-106`）+
`InMemorySessionSink` / `JsonlSessionSink`（`jsonl.rs`）+ typed
`SessionEvent` 16 variant（`events.rs`）+ `fold_surface` 纯函数
（`surface.rs`）+ `interrupted_turn_closers`（`repair.rs`）+ typed
memory 投影（`synthia-context/src/memory/typed.rs`）+ EpochHeader
写入（`controller.rs:629-705`）。`SessionController` 是 per-session
单 run 串行（`controller.rs`）。

**gap 5.1 — agent loop 仍写 legacy envelope**

R4 报告 §9 已知边界："agent loop 内部写侧仍是 legacy envelope
（`ReActLoop` 不自持 SessionSink，持久化归 controller）"。
后果：

- `events_to_messages`（`synthia-context`）的双路径对 legacy 行
  自动回退，但 replay 工具无法用 typed fold 重放纯 R4+ 新写
  的 session——legacy 行无法 replace，永远 append。
- 一个 session 的"前半段 legacy + 后半段 typed"是合法状态
  （R4 报告 §9 标注），但跨这个边界的 `Compaction` replace
  无法落地——`source_event_seqs` 校验
  （`surface.rs` 的 `BadProvenance`）会把任何 source_event_seq < 
  legacy_bound 的 replace 拒绝。

**gap 5.2 — Compaction replace 上游未接线**

R4 的 `SummarizingContextManager`（`synthia-context/src/summarizing.rs`）
做 tool-result LLM 摘要，但摘要结果是**内存改写**
`replace_text_parts(&mut output.content, text)`（`re_act.rs:1388`），
没有写 `SessionEvent::Compaction { from_seq, to_seq, summary }`
（`events.rs` schema 里有这个 variant 但零写入者）。所以 replay
看不到"这一段 tool_result 被摘要替换了"——只有当前 run 的
内存 history 看到新版本。

**gap 5.3 — SessionPreparation / 子 lineage 不在 R4**

dsh 的 `SessionPreparation`（`preparation.ts`）是 Disposable 包装
unpublished session——它把"provider-owned 在 publish 前的 session
状态"与"已发布 session"分开。Synthia 当前没有这个概念：每个
session 一创建就 commit，provider 没有机会 hold 一个"试用 session"
给用户试跑。后果：sub-agent fork 时只能复用同一个 session
（`child_session_id` ULID 铸在 `delegation.rs` 内存），无法做
"预览性 fork + rollback"。

**gap 5.4 — replay 工具是 opqaue JSONL 上的私有投影**

`SessionController::reconstruct_messages_from_session`
（`controller.rs:610-621`）调 `events_to_messages`（typed 投影
+ legacy 投影）。但这是一个 `pub(crate)` 方法（被
server-internal 调用），没有 HTTP 端点暴露——operator 想看
"session abc123 完整 history"必须开 dev shell。pi 提供
`reconstructMessagesFromSession` 是 server-side 私有的（frontend
对应路径同样私），但 traitclaw 暴露了一个 `GET /sessions/:id/messages`
的 HTTP route——这是一个**开发者体验**的明显缺口。

---

### 轴 6 · Developer experience / API / docs

**当前形态**：`crates/synthia-server` 提供：
- `/livez` / `/readyz` / `/metrics`（`routes/health.rs`）
- `/api/models`（`routes/health.rs`）
- `/api/v1/chat/sessions*`（chat route 全集）
- `/api/v1/sessions`（管理面）
- `/api/v1/agents*`（CRUD agents）
- `/api/v1/tools*`（CRUD tools）
- `/api/v1/skills*`（CRUD skills）
- `/api/v1/memory*`（memory route）
- OpenAPI 通过 `utoipa` 生成（`server/router.rs`）
- 4 个 builtin skill（`skill/src/seed.rs`）

**gap 6.1 — Typed event schema 不在 HTTP surface**

`SessionEvent` 的 16 variant + `SurfaceOp` 是 R4 的核心不变量。
当前它们只能通过 `serde_json::Value` 的 opaque JSONL 看到——
`GET /api/v1/sessions/:id/events`（如果存在）只能返回
`Value`。operator 想看"session xyz 是不是有 Compaction 事件"
必须 server-side 跑 typed fold。R5 应暴露：

- `GET /api/v1/sessions/:id/events?from_seq=N&limit=M` 返回 typed
  `SessionEvent` JSON（按 `SessionEvent` enum 的 serde shape）。
- `GET /api/v1/sessions/:id/surface` 返回 `fold_surface` 后的
  `Vec<Message>`，等价 dsh `surface-fold.ts`。

**gap 6.2 — 缺 per-tool replay endpoint**

当前 `ToolRegistry` 的 `version()`（`registry.rs:513-515`）单调计数，
但没有"replay 上一次 tool call"或"列出 session 内所有 tool call
+ result"的 HTTP 端点。traitclaw `crates/traitclaw-eval` 提供
类似 `eval replay` 的能力。Synthia 的 dev shell 才能 dump。

**gap 6.3 — Server / agent 状态 introspection 缺 CLI**

启动 server 后，`/api/v1/agents` 列出 agents、`/api/v1/tools` 列出
tools，但 server 的 runtime 状态——session 数、`usage_meter` 累计、
最近一条 `WarningKind::Hook` ——只能从 `/metrics` 推断。pi 提
供 `mavis status` CLI 直接从 server 拉；Synthia 当前没有 CLI
二进制（只有 `synthia-server`），operator 体验依赖浏览器或
curl。

**gap 6.4 — Docs 漂移：R4 报告 §9 的"已知边界"还没写进 AGENTS.md**

R4 报告 §9 列出 6 条已知边界（agent loop legacy envelope 写入、
Compaction replace 上游未接线、EpochHeader dedup 边界、Phase C 取消
后 sink 引用还在、Phase E 的 lane 抽象、`Steering::wrap_fail_isolated`）
——这些都没有进 `AGENTS.md` 的 §3（编码规范）。后果：未来改
ReActLoop 输出路径时没人知道"还需要把 controller 的 durable
事件分类映射到 `SessionEvent` 变体"——R5 收尾时应同步更新。

---

## 3. 跨轴 R5 采纳候选（按 ROI 排名）

> **排名维度**：
> - **业务影响**：把"agent 在生产环境里能跑 / 不能跑 / 跑得
>   多稳"放在第一权重。
> - **实施成本**：单 crate 内部修改 vs 跨 crate 协调。
> - **现有基础**：已有 typed event / hook fail-isolation /
>   typed retry 等可复用锚点。

| Rank | ID | 轴 | 描述 | 参考 | Effort | 涉及文件 |
|---|---|---|---|---|---|---|
| **1** | **R5-G5.1** | 5 | ReActLoop 写 typed event（消除 legacy envelope），让 typed fold 在所有 session 上生效 | dsh `block-rows.ts` / `known-event-types.ts` | **M** | `synthia-agent/src/agent/re_act.rs`（事件写入路径重写为 `SessionEvent::AssistantMessage` / `ToolCall` / `Step` / `Turn` / `Iteration` / `Warning` / `SteeringGuard` / `SteeringHint` / `HookBlock` / `SubagentEnter` / `SubagentExit` 11 个变体）、`synthia-session/src/events.rs`（已存在 variant 复用）、`synthia-server/src/session/controller.rs`（删除 `events_to_messages` 的 legacy 路径） |
| **2** | **R5-G1.2** | 1 | 把 `ReActLoop` 4 处手检 cancel 收敛为 `EffectGate::admit(invoke)` 模式 | pi `execution/effect-gate.ts`（53 行） | **M** | `synthia-agent/src/agent/re_act.rs`（4 处 `is_cancelled()` 替换为 gate）、`synthia-agent/src/lib.rs` 或新 `synthia-agent/src/effect_gate.rs`（小新模块）、`synthia-steering/src/hook.rs`（`run_hook` 走 gate——hook 调用的取消传播） |
| **3** | **R5-G4.3** | 4 | Hook span 自动开 `tracing::info_span!("hook", name = hook.name(), stage)`；R4 G-2 标的"自动开 span"落地 | pi `startHarnessSpan('hook', { name })` | **S** | `synthia-steering/src/hook.rs` 的 `run_hook`（开 span + `span.record("error", ...)` on panic） |
| **4** | **R5-G5.2** | 5 | `SummarizingContextManager` 写 `SessionEvent::Compaction` 替换原 tool-result（surface fold replace 上游）；agent loop 输出已 typed 后即可对接 | R4 报告 §9 已知边界 #2 | **M** | `synthia-context/src/summarizing.rs`（写 typed Compaction 替换内存改写）、`synthia-agent/src/agent/re_act.rs`（`replace_text_parts` 拆掉，改 `SessionMemory` 记录 event seq） |
| **5** | **R5-G2.1** | 2 | 引入 `FileMutationQueue`（per-canonical-path Promise chain），两个并行 write 工具竞争同一路径自动排队 | pi `harness/tools/file-mutation-queue.ts`（60 行） | **S** | `synthia-tool/src/registry.rs`（dispatch 路径加 mutation queue hook）+ 新 `synthia-tool/src/mutation_queue.rs`（~80 行） |
| **6** | **R5-G4.1** | 4 | `react_loop` span 注入 `session.id` / `turn.id` / `iteration.id` attributes；provider 子 span inherit | pi `telemetry.ts` `startHarnessSpan` 模式 | **S** | `synthia-agent/src/agent/re_act.rs`（`#[instrument(fields(session.id, turn.id, iteration.id))]` + 阶段 set_field）、`synthia-telemetry/src/propagation.rs`（确认跨 spawn 透传） |
| **7** | **R5-G3.1** | 3 | 抽 `BlockAssembler`（80-150 行），把 `re_act.rs` 揉在 sample_once 里的 chunk-to-message 逻辑提到独立模块；可单测 | dsh `llm/src/assembler.ts`（164 行） | **M** | 新 `synthia-provider/src/assembler.rs` + `synthia-agent/src/agent/re_act.rs` `sample_once` 瘦身 |
| **8** | **R5-G6.1** | 6 | 暴露 `GET /api/v1/sessions/:id/events?from_seq=N&limit=M` + `GET /api/v1/sessions/:id/surface`，typed event 直出 | dsh 的 `KNOWN_SESSION_EVENT_TYPES` + `surface-projection.ts` | **S** | `synthia-server/src/routes/sessions.rs`（2 新 handler）、`synthia-server/src/api/v1/mod.rs`（utoipa 新 schema）、`synthia-session/src/events.rs`（已 typed，无新工作） |
| **9** | **R5-G2.3** | 2 | Tool 失败 / panic 计数 + duration histogram Prometheus 化 | 业界 Prometheus tool 模式 | **S** | `synthia-tool/src/registry.rs`（panic path + record_tool_outcome 加 counter / histogram）、`synthia-telemetry/src/metrics.rs`（注册新 metric family） |
| **10** | **R5-G1.3 + G5.3** | 1, 5 | Sub-agent fork 写 typed `SubagentEnter` / `SubagentExit`；为未来的 `SessionPreparation`（preview fork + rollback）留接口 | dsh `preparation.ts` + `subagent/descriptor` 事件 | **L** | `synthia-agent/src/agent/delegation.rs`（写 typed event）、`synthia-session/src/events.rs`（已是 log-only variant，无需 schema 变更）、新 `synthia-server/src/session/preparation.rs`（Disposable trait） |

> **总和**：10 项 / ~10-15 工作日 / 跨 5 个 crate。
>
> **ROI 排序逻辑**：
> - 1-3 优先级最高因为它们**解除 R4 的 4 条已知边界**——R4 报告
>   §9 写了但没动的部分。
> - 4-6 是 typed event 流贯通后的低成本增量。
> - 7-9 是相对独立的工程化补全。
> - 10 是 multi-session / preview-fork 的 forward-compat 留口——
>   当前没有真实用例，先打桩。

---

## 4. 边界与风险登记

| ID | 风险 | 概率 | 影响 | 缓解 |
|---|---|---|---|---|
| R5-R-1 | R5-G5.1（ReActLoop 写 typed event）会破坏 legacy JSONL reader；老 session 文件无法被新 server 读 | 中 | 中 | 保留 `OpaqueSessionSink` 兼容旧格式；启动期检测 v1 行 → 警告；新写只 v2 |
| R5-R-2 | R5-G1.2（EffectGate）让现有 panic / cancel 路径行为变化；`CancellationToken::is_cancelled()` 替换为 gate.admit 抛错 | 中 | 低 | `EffectGate::admit` 的 fallback = `CancellationToken::current()`；保留手检路径以备 hot-fix |
| R5-R-3 | R5-G2.1（FileMutationQueue）增加每路径 lookup 开销（canonical path resolution） | 低 | 低 | cache `canonicalPath` 在 `Context` 上一次；mutation queue key 用 `Arc<PathBuf>` |
| R5-R-4 | R5-G3.1（BlockAssembler）抽出后，provider streaming 行为可能与现有 `re_act.rs::ChunkStateInner` 的去重逻辑分裂 | 中 | 中 | BlockAssembler 的 straggler dedup 严格对当前 re_act 行为做回归测试；先 in-place 抽 fn 后再独立 crate |
| R5-R-5 | R5-G4.1（span attributes）跨 `tokio::spawn` 边界不传 session.id；子 span inherit 失败 | 中 | 低 | 用 `tracing::Span::current().record("session.id", ...)` 而不是 field macro；新增 `Context` 上 attach |
| R5-R-6 | R5-G6.1（新 HTTP endpoints）若 typed fold 对 legacy 行失败，operator 体验比"opaque + 私有投影"更差 | 中 | 低 | 端点接受 `?format=typed|opaque`；缺省 opaque；typed 失败 → 503 + 明确错误 |
| R5-R-7 | R5-G10（SessionPreparation）当前没有任何 caller；提前抽象可能 over-engineering | 中 | 低 | 标 R5+ 候选；R5 只做 typed SubagentEnter/Exit + 接口预留，不做 Disposable |

---

## 5. R5 不做的事（明确边界）

1. ❌ **不引入完整 traitclaw `AgentStrategy` trait**：R3 已分析
   "`Agent` 已覆盖；第二个策略出现再抽"。R5 仍无新策略需求。
2. ❌ **不引入完整 pi `Lane` model + `CompactionSettings`**：R4
   `SessionLane` trait 已预留；R5 不扩 fan-out。
3. ❌ **不引入 dsh `AgentMessage` vs `Message` 分离**：Rust
   无 declaration merging。
4. ❌ **不引入 dsh `KNOWN_SESSION_EVENT_TYPES` 自动生成**：ts
   模式不适合 Rust 手维护。
5. ❌ **不做 CLI 工具**（R5-G6.3）：依赖真实需求；operator 现
   有 /metrics + browser 已可观测。
6. ❌ **不引入完整 dsh `BlockAssembler` 状态机**：只抽出当前
   re_act.rs 的逻辑——避免重写 provider 协议栈。

---

## 6. 参考链接

| 项目 | 关键模块 |
|---|---|
| traitclaw | `crates/traitclaw-core/src/{traits,types,memory,default_strategy,agent_builder,pool,transformers}.rs` |
| dsh | `packages/core/session/src/{types,surface,repair,preparation,request-header,known-event-types}.ts` + `packages/core/agent-loop/src/tool-calls.ts` + `packages/llm/llm/src/{retry-policy,assembler,attribution}.ts` + `packages/llm/token-meter/src/{estimate,attribution}.ts` |
| pi | `packages/agent/src/harness/{hooks,execution/effect-gate,execution/tools,tools/file-mutation-queue,telemetry}.ts` + `packages/agent/src/agent-loop.ts` |
| synthia 当前 | `crates/synthia-{agent,tool,provider,server,session,steering,context,telemetry,skill}/src/**` |

---

## 7. 落地排期建议

按 ROI：

1. **首批**（~3-4 天）：R5-G4.3（hook span 自动开）+ R5-G5.1
   （ReActLoop 写 typed event 解开 R4 边界 #1）+ R5-G2.1
   （FileMutationQueue）——S / M / S 量级，立刻闭环 R4 遗留 + 
   新增 mutation queue 安全不变量。
2. **次批**（~3 天）：R5-G1.2（EffectGate 收 cancel）+ 
   R5-G4.1（span attributes）+ R5-G6.1（typed event HTTP endpoint）。
3. **第三批**（~3-4 天）：R5-G5.2（Compaction replace 上游）+
   R5-G3.1（BlockAssembler 抽出）+ R5-G2.3（tool metrics）。
4. **候选**（按真实用例再排）：R5-G10（SubagentEnter/Exit typed +
   SessionPreparation 接口预留）。

**总工作量**：~10-12 工作日。R5 落地后，Synthia 的
"systemic invariants"（session 不可变 fold、agent/turn/step 边
界、cancel 收敛、mutation queue、typed HTTP event dump）将与
traitclaw / dsh / pi 三家对齐。
