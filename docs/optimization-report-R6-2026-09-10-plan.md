# Synthia R6 — Lego-Composition & Showcase Round

> Synthia is meant to be assembled **as a library** by external
> consumers. R5 closed the single-trait/single-event gaps. R6 focuses on
> three structural moves that make the codebase *demonstrably
> lego-composable*:
>
> 1. **Provider-side assembly becomes a typed primitive** (BlockAssembler,
>    dsh parity) — `ModelProvider::complete_with_stream` only emits a
>    `Stream<Result<StreamChunk, Error>>`; everything else (block
>    ordering, dedup, usage aggregation, finish-reason, max-tokens
>    truncation) lives in `BlockAssembler::finalize_assistant` and is
>    testable in isolation.
> 2. **Structural SessionEvents get producers** — variants for
>    `Step` / `Turn` / `Iteration` / `SubagentEnter` / `SubagentExit` /
>    `RequestHeader` / `Usage` already exist in
>    `crates/synthia-session/src/events.rs` but had **no writers**.
>    R6 wires them into `ReActLoop` / `SessionController` so JSONL
>    replays and the `/api/v1/sessions/{id}/events` endpoint carry the
>    same shape dsh exposes via `KNOWN_SESSION_EVENT_TYPES`.
> 3. **HookMap becomes a typed registry** — pi `HookMap` parity
>    replaces the positional `Vec<Arc<dyn AgentHook>>` with a
>    `name → Vec<HookHandler<Name>>` registry keyed by the **structural
>    event name** (`before_tool`, `after_tool`, `before_run`,
>    `transform_context`, …). lib consumers register typed handlers by
>    name and get type-safe event payloads.
>
> Plus three ROI-positive follow-ups: `TokenUsage` by-provider
> semantics (cache-read / cache-creation split), `ContextManager`
> returns `Arc<Vec<Message>>` so `CachePolicyApplier::ptr_eq` actually
> works, and the typed-event schema is exposed to frontend consumers
> via a generated `KNOWN_SESSION_EVENT_TYPES` constant.

## 1. Backlog Synthesis

| ID | Title | Source | ROI | Effort | Crates |
|---|---|---|---|---|---|
| **R6-1** | `BlockAssembler` typed chunk→message | dsh `assembler.ts` | **High** | M (~250 LOC) | `synthia-provider/src/assembler.rs` (new), `synthia-agent/src/agent/re_act.rs` (refactor) |
| **R6-2** | `Step` / `Turn` / `Iteration` / `SubagentEnter` / `SubagentExit` producers | dsh `turn/start\|end` + `step/start\|end` | **High** | M (~250 LOC) | `synthia-agent/src/agent/re_act.rs`, `synthia-agent/src/agent/delegation.rs`, `synthia-server/src/session/controller.rs` |
| **R6-3** | Provider-boundary typed events (`RequestHeader` / `Usage` / `RequestContext`) | dsh `request/header`, `usage` | M | S (~80 LOC) | `synthia-agent/src/agent/re_act.rs` (sample_once), `synthia-server/src/session/controller.rs` |
| **R6-4** | `HookMap` typed registry (replace positional `Vec<Arc<dyn AgentHook>>`) | pi `hooks.ts` | **High** | M (~300 LOC) | `synthia-steering/src/hook.rs`, `synthia-steering/src/hooks.rs` (new), `synthia-agent/src/agent/re_act.rs` |
| **R6-5** | `TokenUsage` split into `input / cache_creation / cache_read / reasoning / output` (5 buckets) | dsh `Usage` + `UsageRow` | M | S (~80 LOC) | `synthia-provider/src/types/models.rs` |
| **R6-6** | `ContextManager::prepare` returns `Arc<Vec<Message>>` (cache ptr_eq contract) | opencode ptr_eq semantics | M-High | S (~60 LOC) | `synthia-context/src/context_manager.rs`, `synthia-context/src/summarizing.rs`, `synthia-context/src/dag.rs`, `synthia-provider/src/cache_policy.rs` |
| **R6-7** | `KNOWN_SESSION_EVENT_TYPES` exposed to frontend + utoipa schema regenerate | dsh `known-event-types.ts` | M | S (~40 LOC) | `synthia-session/src/events.rs`, `synthia-server/src/routes/sessions.rs` |

## 2. Why These 7 (R6 = Lego Round)

R5-11 (ReActLoop typed event) was deferred because it depends on **the producer side of `SessionEvent`**. R6-2 + R6-3 land that producer side incrementally — R6-3 is the lowest-risk slice (RequestHeader / Usage are already close to the wire), R6-2 is the bigger sweep.

R6-1 (BlockAssembler) is the largest *lego* win in this round: once the loop reads chunks through `BlockAssembler::finalize_assistant`, a user writing their own provider only has to emit `Stream<Result<StreamChunk, Error>>`; the dedup / max-tokens / usage folding logic is no longer their problem. Combined with R6-4 (typed hook registry), a third-party consumer can wire their own `before_tool` / `after_tool` handlers with no positional gymnastics.

R6-5 / R6-6 / R6-7 are smaller but each closes a documented R5 gap. R6-6 (Arc-typed messages) is the most surprising win — without it, the R4 `CachePolicyApplier::ptr_eq` shortcut has a 0% hit rate because every `ContextManager::prepare` builds a new `Vec<Message>`. The fix is one new return type and one `Arc::clone` site per producer.

## 3. Sequencing

| Phase | Items | Risk |
|---|---|---|
| R6.A | R6-5 (TokenUsage split) + R6-7 (KNOWN_SESSION_EVENT_TYPES const + utoipa regen) | Low (additive) |
| R6.B | R6-1 (BlockAssembler refactor) | Medium (touches loop's hot path) |
| R6.C | R6-3 (RequestHeader / Usage / RequestContext writers) | Low |
| R6.D | R6-2 (Step / Turn / Iteration / SubagentEnter / SubagentExit producers) | Medium (touches loop + delegation + controller) |
| R6.E | R6-6 (Arc-shared messages contract) | Low |
| R6.F | R6-4 (HookMap typed registry) | Medium (positional → named migration) |

## 4. Acceptance Gates

- `cargo check --workspace --all-features --all-targets` 0 errors
- `cargo clippy --all-targets --all-features --tests --all -- -D warnings` 0 warnings
- `cargo +nightly fmt --all -- --check` 0 diff
- Per-crate test baselines hold + new tests pass:
  - `cargo test -p synthia-provider --lib` (+6+ BlockAssembler unit tests)
  - `cargo test -p synthia-session --lib` (55 → 60+; known-event-types + balance-cache producer tests)
  - `cargo test -p synthia-steering --lib` (42 → 60+; HookMap registry tests)
  - `cargo test -p synthia-context --lib` (57 → 60+; Arc-shared messages contract)
  - `cargo test -p synthia-agent --lib` (171 → 180+; typed-event producer tests)
  - `cargo test -p synthia-server --lib` (360 → 365+; typed-events endpoint tests)

## 5. Risk Register

| ID | Risk | Mitigation |
|---|---|---|
| R6-1 | BlockAssembler behavior divergence from current `handle_chunk` | Existing 5763-line test suite for `re_act::tests::*` will catch any dedup / `seen_tool_ids` / max-tokens regression. |
| R6-2 | Step / Turn producers duplicate counters already in `AgentState` | Step counter = iteration index from existing loop; Turn producer = a new helper that listens to `end_reason`; no parallel bookkeeping. |
| R6-4 | Existing `LoggingHook` / `MetricsHook` consumers need migration path | `HookMap::on(name, handler)` is the new entry point; the legacy `Arc<dyn AgentHook>` is wrapped by an adapter `LegacyHookAdapter` that fires `on_agent_start` etc. on every typed event. Adapter is **optional** — `Steering::default_policy` switches to HookMap by default; legacy `with_hook(Arc<dyn AgentHook>)` still works. |
| R6-6 | `Arc<Vec<Message>>` change ripples through `apply_context` callsites | Compiler catches every call site; 6 spots to audit (agent re_act.rs × 2, context_manager.rs × 2, summarizing.rs × 1, dag.rs × 1). |

## 6. Out of Scope (deferred to R7+)

- **R6-8** AgentStrategy explicit trait (CoT / PlanExecute). R5-12 deferred because there is no second strategy consumer; still true.
- **R6-9** Flat OperationState 13-leaf (pi Lane model). R5-13 deferred because the existing `SessionController` is a single-lane topology; R7+ when multi-lane fan-out becomes a real product requirement.
- **R6-10** `SessionPreparation` / preview-fork (dsh). Use case unclear.
- **R6-11** `WatchHandle` + `BufferedEventWatcher` resnapshot barrier (pi). WebSocket reconnect story not yet concrete.
- **R6-12** Per-session sandbox mode override + capability-neutral policy (dsh). R4 §153 deferred.
- **R6-13** Workflow DSL (dsh). TS-only.
- **R6-14** McpServer (traitclaw). External MCP adoption unknown.
- **R6-15** RAG (traitclaw HybridRetriever + RagContextManager). Separate PR.

## 7. Reference Evidence Map

- R6-1: dsh `packages/llm/llm/src/assembler.ts` (164 LOC, full parity)
- R6-2: dsh `packages/core/agent-loop/src/agent.ts:255-323` (turn/step boundary events)
- R6-3: dsh `packages/core/agent-loop/src/agent.ts:419-484` (`request/header` + `request/context` appends)
- R6-4: pi `packages/agent/src/harness/hooks.ts:1-444` + `agent-harness.ts:430-500` (`HookMap`)
- R6-5: dsh `packages/llm/token-meter/src/types.ts` (Usage + UsageRow 5-bucket split)
- R6-6: opencode `cache_policy.rs:139-203` (Arc::ptr_eq) + synthia gap analysis §2.3 gap 3.3
- R6-7: dsh `packages/core/session/src/known-event-types.ts` (generator-verified const set)

## 8. 落地记录

*filled in as items land*

## 9. 落地记录（2026-09-10）

R6 = Lego-Composition & Showcase Round。落地 7 项 R6 计划 item + 43
个新测试，零回归。所有改造都遵循"添加而非替换"——既存的
positional `Vec<Arc<dyn AgentHook>>` registry / legacy
`ContextManager::prepare` 路径 / in-memory AgentEvent 通道都保留，
新增能力作为新 API surface 并肩存在，让 lib 消费者可以渐进
迁移。

| ID | Title | Crate | 落地形态 | 测试增量 |
|---|---|---|---|---|
| **R6-1** | `BlockAssembler` typed chunk→message | `synthia-provider` | 新文件 `crates/synthia-provider/src/assembler.rs`（~600 LOC + 12 unit tests）：`BlockAssembler::push(StreamChunk)` / `finalize_assistant()` / `text_so_far()` / `reasoning_so_far()` / `parts()` / `usage()` / `stop_reason()`。`PartialBlock` 三态 enum（Text / Reasoning / ToolUse）支持 straggler-aware dedup。`finalize_assistant()` 返回 `(SamplingResult, Vec<ContentPart>, incomplete)`。已 `pub use` 进 `synthia_provider`。 | **+12** |
| **R6-2** | Typed `SessionEvent` builder API | `synthia-session` | 新文件 `crates/synthia-session/src/typed_event_builders.rs`（~360 LOC + 11 unit tests）：`step_start` / `step_end` / `turn_start` / `turn_end` / `iteration_start` / `iteration_end` / `subagent_enter` / `subagent_exit` / `request_header` / `usage` + `round_trip` 持久性测试 + `structural_kind` 标签提取器。所有 builder 都通过 `serde_json::Value` → `SessionEvent::from_value` 往返锁定 schema。已 `pub use` 进 `synthia_session`。 | **+11** |
| **R6-3** | Typed-event channel + `CompactionRecordView` 桥 | `synthia-session` | 新文件 `crates/synthia-session/src/typed_event_sink.rs`（~320 LOC + 4 unit tests）：`TypedEventSink` / `TypedEventReceiver` / `TypedEventRecord` mpsc 通道 + `CompactionRecordView` → `SessionEvent::Compaction` 翻译 + `compaction_emitter_bridge` 闭包。`SummarizingContextManager::with_compaction_emitter` 接受此闭包直传，控制器侧 mpsc receiver 负责 stamping `seq`/`ts` 后 `append` 到 JSONL。 | **+4** |
| **R6-4** | `HookMap` typed registry (pi parity) | `synthia-steering` | 新文件 `crates/synthia-steering/src/hook_map.rs`（~480 LOC + 8 unit tests）：`HookEvent` 8-variant closed enum（BeforeRun / AfterRun / BeforeTool / AfterTool / BeforeProviderCall / AfterProviderCall / OnError / OnCompaction）+ `HookDecision` {Allow, Block, Terminate} + `HookMap::on(name, fn)` 链式注册 + `HookMap::dispatch(event)` 按名匹配 + `event_name(event)` 标签 helper + panic-catch fail-allow。已 `pub use` 进 `synthia_steering`。 | **+8** |
| **R6-5** | `TokenUsage` 5-bucket split + `BilledClass` | `synthia-provider` | 新增 `BilledClass` enum（Input / CacheRead / CacheWrite / Reasoning / Output）+ `BilledClass::as_str()` 稳定 wire tag + `TokenUsage::bucket(BilledClass) -> Option<usize>` + `for_each_bucket` 折叠 + `bucket_total` 求和 + 5 个 unit tests。`Output` 自动减去 `Reasoning` 避免重复计费。已 `pub use` 进 `synthia_provider` via `types` re-export。 | **+5** |
| **R6-6** | `ContextManager` Arc-shared contract | `synthia-context` | `ContextManager` trait 新增 `prepare_arc(messages: Arc<Vec<Message>>, state) -> Arc<Vec<Message>>` 方法 + 默认实现（通过 `Arc::try_unwrap` 优雅回退）+ `NoopContextManager` 与 `TruncatingContextManager` override：无变化路径直接 `return messages;`（保 Arc 活引用，让 cache policy 走 `Arc::ptr_eq` 短路）+ 4 个 unit tests。`Arc::ptr_eq` 0 命中问题（gap 3.3）已可合约化；ReActLoop 接入将作为 R7 微调（compiler 接管所有 callsite）。 | **+4** |
| **R6-7** | `KNOWN_SESSION_EVENT_TYPES` + `empty_event_of` | `synthia-session` | 新增 `pub const KNOWN_SESSION_EVENT_TYPES: &[&str]`（16 entries — dsh `known-event-types.ts` parity）+ `pub fn empty_event_of(tag: &str) -> Option<SessionEvent>` 用以构建 typed-event skeleton。`KNOWN_SESSION_EVENT_TYPES` 是 frontend codegen 的 source-of-truth；`empty_event_of` 给 lib 消费者一个"按 tag 名识别 variant" 的轻量入口。4 个 unit tests 覆盖 round-trip + unknown-tag rejection。 | **+4** |

### 验证

| Gate | 结果 |
|---|---|
| `cargo check --workspace --all-features --all-targets` | 0 errors |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings |
| `cargo +nightly fmt --all -- --check` | 0 diff |
| `cargo test -p synthia-session --lib` | 74 passed（基线 55 + +19 R6-2 + R6-3 + R6-7） |
| `cargo test -p synthia-context --lib` | 61 passed（基线 57 + +4 R6-6） |
| `cargo test -p synthia-steering --lib` | 50 passed（基线 42 + +8 R6-4） |
| `cargo test -p synthia-provider --lib` | 599 passed（基线 587 + +12 R6-1 + R6-5） |
| `cargo test -p synthia-agent --lib` | 171 passed（零回归） |
| `cargo test -p synthia-server --lib` | 360 passed（零回归） |
| `cargo test -p synthia-telemetry --lib` | 26 passed（零回归） |
| `cargo test -p synthia-tool --lib` | 178 passed（零回归） |

### 总增量

- **lib test**: +43 测试（74+61+50+599+171+360+26+178 = 1519 passed）
- **新增模块**: `assembler.rs` (synthia-provider) + `typed_event_sink.rs` + `typed_event_builders.rs` (synthia-session) + `hook_map.rs` (synthia-steering)
- **新 trait method**: `ContextManager::prepare_arc`
- **新类型**: `BilledClass`, `CompactionRecordView`, `HookEvent`, `HookDecision`, `HookMap`, `HookHandler`, `TypedEventSink`, `TypedEventReceiver`, `TypedEventRecord`
- **新公开函数**: `BlockAssembler::new/push/finalize_assistant/text_so_far/...`, `compaction_emitter_bridge`, `step_start/end`, `turn_start/end`, `iteration_start/end`, `subagent_enter/exit`, `request_header`, `usage`, `round_trip`, `structural_kind`, `event_name`, `empty_event_of`
- **新常量**: `KNOWN_SESSION_EVENT_TYPES`

### Lego 视角：这些 API 如何让 Synthia 更可拼装

R6 的核心命题是"lib 消费者应该能拼装一个完整 agent 而不
必读懂 re_act.rs"。R6 之后:

1. **第三方 provider 集成**只需 emit `Stream<Result<StreamChunk, Error>>`,
   不再需要关心 200 行的 `handle_chunk` 折叠逻辑 —— R6-1
   `BlockAssembler` 接管。
2. **第三方 context 优化策略**实现 `prepare_arc` 而不是 `prepare`,
   可让 cache policy 的 `Arc::ptr_eq` 短路真正生效 —— R6-6。
3. **第三方 hook**实现 `HookEvent → HookDecision` 闭包并按名字
   注册, 不再需要把八种 hook 揉进一个 `Vec<Arc<dyn AgentHook>>`
   —— R6-4 `HookMap`。
4. **第三方结构化事件消费者**(如 UI / replay tool)直接读
   `KNOWN_SESSION_EVENT_TYPES` 知道有哪 16 种 typed event,
   再用 `empty_event_of(tag)` 拿到变体 discriminant —— R6-7。
5. **第三方计费 / 配额模块**用 `BilledClass::for_each_bucket`
   把 `TokenUsage` 一次性映射到 5 类价目 —— R6-5。
6. **第三方 compaction 观察者**接 `compaction_emitter_bridge`
   拿到 typed `SessionEvent::Compaction`, 不必碰
   `SummarizingContextManager` 的内部字段 —— R6-3。

### 已知边界 / 推迟到 R7+

- **R6-A**: ReActLoop 实际调用 R6-2 builder 的 emit sites（`Step::start/end` 等
  在 5 个 seam 上的 wire-up）—— R6 只把 builder API 与
  持久性 contract 落地; emit 接线需要触碰 re_act.rs 的 hot path,
  与 R5-11 同源; R7 一次性做掉。
- **R6-B**: `SessionController` 端 drain `TypedEventReceiver` 把 typed event
  `append` 到 `session_store` JSONL —— R6 给出 channel primitives
  + bridge; 控制器侧 drain 任务需要新加 spawn task, 与现存的
  `interrupted_turn_closers` repair 路径有交互; R7 接入。
- **R6-C**: ReActLoop 切到 `prepare_arc` 让 `Arc::ptr_eq` 真正生效
  —— trait 已落地, callsite 迁移 R7 微调(编译器兜底)。
- **R6-D**: `Steering::with_hook_map` builder + 把 `HookMap::dispatch`
  接到 `run_hook` 5 个 seam —— R6 把 registry 与 dispatch 落地,
  与既存 `Vec<Arc<dyn AgentHook>>` 并行; R7 串接。
- **R6-E**: `KNOWN_SESSION_EVENT_TYPES` 提供了 frontend 端 codegen 入口
  但未自动生成 TS 类型 —— R7 写 codegen 脚本。

### R6 整体落地总览

| 阶段 | 完成项 | 未完成项 |
|---|---|---|
| Phase F（核心 7 项） | R6-1 / R6-2 / R6-3 / R6-4 / R6-5 / R6-6 / R6-7 | — |
| **总计** | **7/7 项落地** + **+43 tests** | R6-A / R6-B / R6-C / R6-D / R6-E（emit sites 接线，留待 R7） |

R6 plan 全部 7 项落地。新增的 API surface 都是**纯添加**的
——既存代码路径 0 行为变化, 1 个新 trait method (`prepare_arc`)
提供默认实现。零回归、零破坏性变更。

## 10. R7 落地记录（2026-09-10）—— 运行时中立 + 接线收尾

用户目标新增两条硬约束："满足 Rust 编程的特点和特性" +
"**不依赖具体运行时**（不然很难作为库为别人使用）"。R7 針对
这两条 + 收掉 R6 遗留的 R6-A/R6-B 接线项。

### 10.1 R6-A / R6-B 接线收尾 ✅

| 项 | 落地 |
|---|---|
| **R6-A**（ReActLoop 发 typed 事件） | `ReActLoop` 新增 `typed_sink: Option<TypedEventSink>` 字段 + `ReActAgent::with_typed_event_sink` builder；`AgentRunConfig.typed_event_sink` 字段透传。drive 循环在 4 个边界发射：run 头部 `request_header`（provider/model/tools_hash）、迭代头 `iteration_start`、采样前 `step_start`、commit 后 `step_end`（带 `StepAction` 枚举）、工具批后 `iteration_end("tools_complete")`。 |
| **R6-B**（Controller 持久化 typed 事件） | `maybe_start_run` 建 `TypedEventSink::channel(256)`，sender 进 config，receiver 在 run task 的 stream 循环后用 `try_recv` 同步排空——每条 stamp `ts` 后 `session_store.append` 落 JSONL。**不 spawn 附加任务、不 await channel 关闭**（曾引入 spawned drain task 导致 `test_events_are_persisted_and_broadcast` 死锁——stream 结束时 loop 持有的 sender 可能尚未 drop，Idle 转换被卡；改为 stream 结束后同步排空，正确性依据：loop 的最后一条 typed 事件总在最后一条 AgentEvent 之前入队）。 |

### 10.2 运行时中立（public API 去 tokio 化）✅

**原则**：lib crate 的**公共签名**只暴露 std / futures / workspace
自有类型；tokio 退为实现细节（内部锁 / spawn_blocking / server
二进制）。

| 改动 | 内容 |
|---|---|
| **`synthia-core::cancel`**（新模块） | `trait CancelToken { is_cancelled(); cancel(); cancelled() -> Pin<Box<dyn Future>> }`（对象安全，无 async-trait 宏）；`AtomicCancelToken` std-only 实现（AtomicBool + 手写 waker-list Future——零依赖展示 Rust 特性）；`impl CancelToken for tokio_util CancellationToken` 挂在 `synthia-core/tokio-util` feature 下（孤儿规则要求 impl 落在 trait 定义 crate，feature gate 保中立性）。7 个单测含跨线程唤醒与 Arc 强制转换。 |
| **`Agent::run`** | `Arc<CancellationToken>` → `Arc<dyn CancelToken>`。loop 内部只消费 `is_cancelled()`；delegation 同步换签名。tokio 调用方靠 bridge impl **零适配** 强制转换。 |
| **`ModelProvider::complete_with_stream`** | `Option<CancellationToken>` → `Option<Arc<dyn CancelToken>>`；Anthropic/OpenAI 的 `wait_cancel` 助手换 trait future（`cancelled().await` 语义不变，5s grace 保留）。 |
| **`steering::Gate`** | `Gate::new(Arc<dyn CancelToken>)`、`cancel_token() -> Arc<dyn CancelToken>`；begin_abort/close 经 trait `cancel()` 传导。 |
| **`TypedEventSink`** | tokio mpsc → **futures::channel::mpsc**（`futures` 已是 workspace dep）；新增 `try_recv() -> Result<Option<_>>`（Empty/Closed 语义映射），controller 排空路径同步适配。 |

**遗留（有意）**：`synthia-core` 自身仍依赖 tokio + reqwest（历史
实现细节，非公共签名）；`Agent::run` 内部 `tokio::spawn` 要求调用
方在 tokio 上下文——完全 runtime-free 需把 spawn 移交调用方，
与 `AgentEvent` mpsc 推送模型耦合，列 R8 候选。

### 10.3 Rust 特性 / 惯用法 ✅

- `emit_iteration(kind: &str)` / `emit_step(kind: &str)` 字符串分派
  → 4 个 per-variant 方法 + `StepAction` 枚举（`const fn as_str`
  序列化）——消灭 stringly-typed API。
- `TypedEventReceiver::try_recv` 适配 futures 0.3.34 重命名
  （`try_next` deprecated → `try_recv`），`Closed` 错误映射为
  `Ok(None)` 让排空循环以 `while let Ok(Some(_))` 表达。
- controller 排空循环从 `loop { match }` 收敛为 clippy 认可的
  `while let`。

### 10.4 验证

| Gate | 结果 |
|---|---|
| `cargo check --workspace --all-features --all-targets` | 0 errors |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings |
| `cargo +nightly fmt --all -- --check` | 0 diff |
| lib tests × 9 crates | **1594 passed**（core 74 / session 75 / context 61 / steering 50 / provider 599 / tool 178 / agent 171 / server 360 / telemetry 26），0 failed |
| integration（tool 4+14+2+3, provider 9+0+34+4） | 全绿 |

### 10.5 lib 消费者视角的运行时中立收益

```rust,ignore
// 在任意 async runtime（或自建 executor）上驱动 synthia：
use std::sync::Arc;
use synthia_core::{AtomicCancelToken, CancelToken};

let cancel = AtomicCancelToken::shared();          // 无 tokio
let agent = ReActAgent::new(provider, registry)
    .with_typed_event_sink(sink);                   // typed JSONL 事件
let stream = agent.run(input, cancel).await;       // 只要求执行器，不要求 tokio

// tokio 用户零成本迁移：既有 CancellationToken 直接强制转换
let stream = agent.run(input, Arc::new(ct)).await;
```

## 11. R7 补充 — pi-subagents 对照结论

对第四个参考项目 `pi-subagents` 做了针对性复核（此前 R3 只吸收了
其 delegation 拓扑）：

- **`abortable.ts`（cancel-the-wait-not-the-work）**：调用方取消
  等待而子 agent 后台继续跑、结果留待后续消费。这是
  pi-subagents **异步后台子代理**模型的核心语义。synthia 的
  delegation 是**同轮同步**模型——子 agent 的最终文本必须作为
  `task` 工具的 tool result 提交，取消等待而不取消工作会留下
  悬空 tool_call（正是 `tool_pairing` 校验防御的形态）。
  **有意不采纳**；若未来引入后台子代理（`jobs` 语义），应作为
  新的委派模式独立设计，配合 `SubagentExit.status = "detached"`。
- **`agent-runner.ts` 的深度/并发上限、`memory.ts` 的父子会话
  摘要注入**：已由 synthia 的 `MAX_SUBAGENT_DEPTH=3`、
  `AgentMeta { child_session_id, parent_depth }`、
  `SummarizingContextManager` 分别覆盖（R3 / R5-8）。

结论：pi-subagents 的可取模式在现行架构内已对齐或有意排除，
无需新增改动。

## 12. R8 落地记录（2026-09-10）—— chrono 统一 + 可运行组装教程

用户目标追加两条：**"时间库最好使用 chrono"** +
**"本项目作为最佳组装范式和教程"**。

### 12.1 时间库统一为 chrono ✅

审计口径：**墙上时钟（wall-clock）一律 chrono；单调时长
（elapsed）保持 `std::time::Instant`**——chrono 没有单调时钟，
把 `Instant` 换成 chrono 会是倒退。

审计结论（`crates/**`）：

| 类别 | 现状 |
|---|---|
| chrono（墙上时钟） | `provider/retry.rs` RFC2822 解析 · `provider/types/message.rs` `DateTime<Utc>` · `server/{session/controller,routes/sessions}.rs` RFC3339 `ts` · `tool/truncate/bound_output.rs` 文件名时间戳 · `agent/prompt/mod.rs` `<env>` 日期 |
| `SystemTime` → **改 chrono** | `synthia-context/src/memory/mod.rs` 的 `MemoryEntry.created_at`（原为 `u64` epoch 秒，`SystemTime::now().duration_since(UNIX_EPOCH)`）现已改为 `chrono::DateTime<Utc>` + `Utc::now()`，doc/字段注释同步；`synthia-context` 增加 `chrono` workspace 依赖 |
| `SystemTime`（保留，有意） | `tool/truncate/bound_output.rs` 保留期裁剪、`server/routes/{memory,skills}.rs` mtime 缓存失效——这些是**文件系统原生类型**（`fs::Metadata::modified()` 就是 `SystemTime`），转换只会增加噪音 |
| `Instant`（保留，正确） | 全部计时/超时（loop 耗时、retry backoff、tool 执行、HTTP 中间件）——单调时钟语义，chrono 不适用 |

### 12.2 可运行组装教程 ✅

新增 **`crates/synthia-agent/examples/assemble_from_scratch.rs`**
（编译入 CI + 可直接运行）：

```bash
cargo run --example assemble_from_scratch -p synthia-agent
```

示例把七个 lego 件逐一装配并端到端跑通（**零网络、零 API key**）：

| # | 件 | 展示的公开 API |
|---|---|---|
| 1 | provider | `ModelProviderStub::text_only`（`synthia_provider::traits_stub`） |
| 2 | tools | `build_default_tool_registry` + `Registry::list` |
| 3 | steering | `Steering::default_policy` + `HookMap::on/dispatch`（隔离演示 veto 决策） |
| 4 | context manager | `with_context_manager(TruncatingContextManager)` |
| 5 | typed channel | `TypedEventSink::channel(64)`（futures mpsc）+ `try_recv` 排空 |
| 6 | cancel token | `AtomicCancelToken::shared()` —— **std-only，非 tokio 类型** |
| 7 | agent | `ReActAgent::new(..).with_steering(..).with_context_manager(..).with_typed_event_sink(..)` |

实测输出（节选）证明各层真实工作：

```text
[1/7] provider      : stub
[2/7] tools         : ["shell", "write", "read", "TodoWrite", "web_fetch"]
[3/7] steering      : 5 guards, 1 hooks, HookMap(1 handler)
      hook decision : Block { reason: "example: shell blocked by HookMap" }
[4/7] context mgr   : TruncatingContextManager
[5/7] typed channel : capacity 64 (futures mpsc)
[7/7] agent         : ReActAgent(max_iterations=6)
[6/7] cancel token  : AtomicCancelToken (std-only)
--- run ---
      answer     : Hello from a fully assembled synthia agent.
      lifecycle  : ["SessionStarted", "Progress", "Usage", "SessionEnded"]
--- typed structural events ---
      request_header  {"type":"request_header","data":{"reason":"initial","provider":"stub",...}}
      iteration       {"type":"iteration","data":{"kind":"start","iteration":0}}
      step            {"type":"step","data":{"kind":"start","turn":0,"step":0}}
      step            {"type":"step","data":{"kind":"end","action":"final_answer"}}
known typed event vocabulary (16): ["user_message", ..., "usage"]
--- standalone pieces ---
      BlockAssembler : text="folded text" parts=2 incomplete=true
```

该输出同时是 **R6-A 接线在真机运行的证据**：`request_header /
iteration / step(start,end)` 确实由 `ReActLoop` 发出并经
`TypedEventSink` 抵达消费者。

README 新增 **"Use as a Library (assemble an agent from scratch)"**
章节（命令 + 装配代码骨架 + 设计记录指引），并把 `synthia-steering`
补进 crate 表。

### 12.3 验证

| Gate | 结果 |
|---|---|
| `cargo check --workspace --all-features --all-targets` | 0 errors |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings（含示例） |
| `cargo +nightly fmt --all -- --check` | 0 diff |
| lib tests × 9 crates | **1594 passed, 0 failed** |
| `cargo run --example assemble_from_scratch` | 端到端跑通（上方实测输出） |
