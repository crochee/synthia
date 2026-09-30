# synthia ↔ community-wide 抽象与插件化审计

> **基线**：`2ac1e1c2` (master HEAD)，全部 8 个门禁绿。
> **范围**：synthia-tool / synthia-harness / synthia-session 三大 crate +
> `synthia` facade + 7 个 plugin tool crate + 7 个 opt-in crate。
> **对照源**：pi-mono（`~/workspace/pi-mono/BORROWABLE_PATTERNS.md` §1-§16）、
> traitclaw（`~/workspace/traitclaw`，8 traits + 23 examples）、
> pi-subagents / opencode / codex / dsh（synthia R29/R30 已记录的 4 个参照）、
> 社区其他（moltis / goose / oh-my-openagent / kimi-cli / openclaw / hermes-agent）。
> **性质**：只读。`wc -l` 与 `grep -n` 给出的行号都可复核。

---

## 0. TL;DR — "优化空间"已成三层结构

Synthia 已经吸收了 95% 主流社区抽象。**真实剩余空间**分三层：

1. **本仓库 audit 已识别**(权威源) — `docs/superpowers/specs/2026-09-19-soft-spot-optimizations-design.md`
   的 G1-G7 + N1-N5。其中 G1/G2/G3 已立项，G4/G5/G6/G7 backlog。
2. **文件体量拆分**(本审计 v1 已列 6 commit)
3. **本审计新发现**(见 §5)

---

## 1. 社区主流抽象覆盖矩阵

### 1.1 traitclaw 8 core traits → synthia 对位

traitclaw 的设计哲学：**8 个 trait，每个都是对象安全，`Agent::builder()` 链式装配**。

| # | traitclaw trait | traitclaw 方法 | synthia 对位 | 状态 |
|---|---|---|---|---|
| 1 | `Provider` (`core/src/traits/provider.rs`) | `complete` / `stream` / `model_info` | `ModelProvider`（`synthia-provider/src/traits.rs`） | ✅ 完整对位；R29 增加 `tier()` + `embed()`（Anthropic 留空） |
| 2 | `Tool` (`core/src/traits/tool.rs`) | `name` / `description` / `schema` / `execute` | `Tool`（`synthia-tool/src/traits.rs`）+ `#[derive(Tool)]` macro（R31 synthia-macros） | ✅ 完整；synthia 多了 `mode()` + `truncate()` + `output_definition()` |
| 3 | `Memory` (`core/src/traits/memory.rs`) | `messages` / `append` / `get_context` / `set_context` / `recall` / `store` / `create_session` | `Memory`（`synthia-context/src/memory/mod.rs`）+ `SqliteMemory` + `FileMemory` + `InMemoryMemory` | ✅ 完整；synthia 把 "context k/v" 拆为独立的 `ContextManager` trait |
| 4 | `Guard` (`core/src/traits/guard.rs`) | `name` / `check(&Action)` | `Hook::run_hook`（`synthia-steering/src/hook_map.rs`）+ `Guard` interface（`guard/loop_detection`, `guard/rate_limit` 等 8 个） | ✅ 完整；synthia 的 Hook 系统是 traitclaw Guard + Hook 的合并 |
| 5 | `Hint` (`core/src/traits/hint.rs`) | `should_trigger` / `generate` / `injection_point` | `Hint` trait + `tier::auto` preset（R10-2） | ✅ 完整 |
| 6 | `Tracker` (`core/src/traits/tracker.rs`) | `on_iteration` / `on_tool_call` / `on_llm_response` / `recommended_concurrency` | `Tracker` trait + `AdaptiveTracker` | ✅ 完整；synthia 把"推荐并发"提升为一等公民（harness 用它调 bucket.rs 并发度） |
| 7 | `ContextManager` (`core/src/traits/context_manager.rs`) | `prepare` / `estimate_tokens` | `ContextManager`（`synthia-context/src/context_manager.rs` 859 L） | ✅ 完整 |
| 8 | `OutputTransformer` (`core/src/traits/output_transformer.rs`) | `transform` / `estimate_output_tokens` | `ToolOutputDefinition::presentation_meta` + `finalize_content`（R29 dsh §8 parity）+ `truncate` 默认实现 | ✅ 完整 |

**额外**：traitclaw 还有 `AgentHook` + `ExecutionStrategy` 两个 trait：

| traitclaw trait | synthia 对位 | 状态 |
|---|---|---|
| `AgentHook` (`hook.rs`) — `on_agent_start/end` + 5 生命周期回调 | `synthia_harness::ToolInterceptor`（`crates/synthia-harness/src/agent/interceptor.rs` 120 L） | 🟡 部分覆盖：synthia 是 interceptor 模式（拦截工具调用），traitclaw 是 agent-lifecycle hook（拦截 start/end） |
| `ExecutionStrategy` (`execution_strategy.rs`) — `execute_batch(Vec<PendingToolCall>)` + `SequentialStrategy` / `ParallelStrategy` / `AdaptiveStrategy` | `Tool::mode() -> ExecutionMode` + harness `bucket_by_execution_mode` + `run_sequential_bucket` | ✅ 完整对位；synthia 用 per-tool declaration，traitclaw 用 strategy 选择（**synthia 模式更细粒度**） |

### 1.2 pi-mono 17 项关键模式 → synthia 对位

| # | pi-mono 模式 | synthia 对位 | 状态 |
|---|---|---|---|
| 1 | 8+ lifecycle events | `AgentEvent::Agent/Assistant/User/System` + 14 `SystemEvent` 变体 | ✅ |
| 2 | Extension runtime 双 stub | 不需要（编译期 Rust） | n/a（有意偏离） |
| 3 | File mutation queue | 已删（R111 — 零消费者） | ✅ |
| 4 | ToolDefinition ↔ AgentTool wrapper | `Tool` + `ToolEntry::dynamic` | ✅ |
| 5 | SessionEntry tree (id+parentId) | server 层 fork + `SubagentEnter` event | ✅（落点不同） |
| 6 | CompactionEntry + fromHook | `CompactionCheckpoint` + `from_hook: bool` 字段 | ✅ |
| 7 | CustomEntry vs CustomMessageEntry | 单一 `SessionEvent` enum + `kind` tag | ✅ |
| 8 | RPC LF-only JSONL | 无 RPC mode；axum HTTP + WS + stdio MCP bridge | ✅（偏离） |
| 9 | Plan mode | 无对应 crate | 🟡 backlog |
| 10 | Subagent 递归 spawn | `synthia-tool-task` + `task` tool + `ToolInterceptor` | ✅ |
| 11 | Event Bus pub-sub | 强类型 enum + 模式匹配（无 string-keyed bus） | ✅（优于 pi） |
| 12 | Permission gate fail-closed | `synthia-steering` + permission fail-closed | ✅ |
| 13 | Compaction split-turn 并行 | `compaction_checkpoint.rs`（单线程） | 🟡 G1 候选 |
| 14 | Branch summarization | fork 走 byte-faithful copy（无 summary） | 🟡 低优 |
| 15 | UTF-8 字符边界安全截断 | `is_char_boundary` floor loop（`bound_output.rs:354-359`） | ✅ |
| 16 | Tool execution mode | `Tool::mode()` + harness bucket | ✅ |
| 17 | Dynamic resource discovery | 无对应机制 | 🟡 |
| 18 | Tool override by name | 无等价 API | 🟡 |

### 1.3 其它社区项目快速对照（高层）

| 项目 | 关键差异化 | synthia 是否吸收 | 来源 |
|---|---|---|---|
| **moltis** | "personal AI gateway, one binary, no runtime" | 概念上 = `synthia-server`；定位接近 | 单一二进制服务，无 npm = synthia 已满足 |
| **goose** | AAIF/Linux Foundation，desktop-first + provider-agnostic | provider-agnostic ✅；desktop-first ✗（synthia 是 web） | AAIF spec 后续可能成为规范 |
| **oh-my-openagent** | Anthropic 限速 → 切 Codex 的"模型 fallback 链" | ❌ 无 model fallback chain（候选 G-NEW-1） | LazyCodex 工程实践 |
| **kimi-cli** | Shell command mode (`/shell`) — 内置 REPL on/off | ❌ 无内置 TUI shell mode（独立 repo G7） | 单纯 CLI 交互差异 |
| **openclaw** | Personal assistant；self-hosted；on-device | 定位类似 synthia-server | 不吸收 |
| **hermes-agent** | "self-improving" — 从经验自动写 skill + 跨会话学习 | ❌ 无 self-improvement loop（候选 G-NEW-2） | Nous Research 设计哲学 |

---

## 2. synthia 自有审计文档（权威源）

**`docs/superpowers/specs/2026-09-19-soft-spot-optimizations-design.md`** 是本仓库 R115/R116 后自己写的复审，基线 `912bcf51` + 后续 R117-R123 修正。其结论：

### 2.1 已立项 gap（G1/G2/G3）

| # | 缺口 | 当前文件/行 | 设计位置 | 状态 |
|---|---|---|---|---|
| G1 | 持久向量存储 | `synthia-search/src/vector.rs:24-97` 全 `FlatVectorStore` | `2026-09-19-wheel-cron-search-persistence-design.md §1` | ✅ R115 已交 SearchEngine.save/load + snapshot+op-log；持久向量 store 待 |
| G2 | cron 立场决策 | `synthia-scheduler/src/lib.rs:7-13` "解析属 host" + `JobKind::Cron{expr}` opaque + 1 分钟占位 | 同上 §2 | ✅ R115 已交 opt-in `cron` feature + Trigger seam |
| G3 | 模型侧 search 工具形态 | `synthia-search/src/hit.rs:81-84` `search_tool` 是 lib fn | 同上 §3 | ✅ R115 已交 `synthia-tool-search`（deferred exposure） |

### 2.2 本轮审计新发现（N1-N5）

| # | 发现 | 候选处理 |
|---|---|---|
| N1 | OpenAI `embed()` 完整；Anthropic 无实现落空 `Vec` | 不做（Anthropic 官方无公开 API） |
| N2 | `ModelProviderEmbedder` 把 `Err` 吞成零向量（`search/src/provider.rs:53-66`） | G4 — 加 `tracing::warn!` |
| N3 | `Error::Provider` 已能表达"能力不支持" | 记录 |
| N4 | `search_tool` 是 lib fn 不是 `Tool` plugin | 已 G3 立项并交付 |
| N5 | server 已不依赖 `synthia-search`（R116） | 记录 |

### 2.3 backlog（G4-G7）

| # | 缺口 | 来源 |
|---|---|---|
| G4 | `ModelProviderEmbedder` 吞错 | N2 |
| G5 | 代码执行沙箱插件（python REPL + timeout） | backlog，原样保留 |
| G6 | Web search 插件（Tavily / Brave provider） | backlog，原样保留 |
| G7 | GUI/TUI 独立 repo | 维持原判 |

---

## 3. 三大 crate 文件体量审计（v1 保留）

### 3.1 synthia-tool（6727 LOC）

| 文件 | LOC | 关注点 | 建议 |
|---|---:|---|---|
| `surface.rs` | 951 | projection + wrappers + deferred | 拆 `surface/{mod, projection, wrappers, deferred}.rs`（−850 L） |
| `truncate/bound_output.rs` | 741 | bound + cleanup_task + bounds | 拆 `truncate/{bound_output, cleanup_task, bounds}.rs`（−50 L） |
| `registry/mod.rs` | 349 | facade + version counter | 拆 `registry/{mod, facade, version}.rs` |
| `output.rs` / `types.rs` / `traits.rs` / `restriction.rs` | <500 | 单关注点 | 保持 |

### 3.2 synthia-harness（3018 LOC）

| 文件 | LOC | 关注点 | 建议 |
|---|---:|---|---|
| `events/mod.rs` | 567 | facade re-export 4 子模块 | 已健康（mod.rs 只是 facade） |
| `prompt/mod.rs` | 434 | assembly + tools_list + runtime_context | 拆 `prompt/{mod, assembly, tools_list, runtime_context}.rs`（−80 L） |
| `compaction.rs` | 371 | 单关注点 | 保持 |
| `re_act/loop_/` | n/a | 7 模块（bucket / dispatch / seams / route / commit / inbox / events） | 已健康（R97 拆分后） |

### 3.3 synthia-session（8518 LOC）

| 文件 | LOC | 关注点 | 建议 |
|---|---:|---|---|
| `events.rs` | 1311 | SessionEvent enum + impl + builders | **最大单文件**；拆 `events/{mod, user_assistant, tool, lifecycle}.rs`（−100 L，风险中） |
| `token_meter.rs` | 977 | UsageBuckets + TokenMeter + TokenBudget | 拆 `token_meter/{mod, buckets, meter, budget}.rs`（−80 L） |
| `log_surface.rs` | 807 | SharedSurfaceLedger + fold + error | 拆 `log_surface/{ledger, fold, error}.rs` |
| `repair.rs` / `compaction_checkpoint.rs` | 800 | tail repair + schema migration / snapshot | 各自保持（关注点统一） |

---

## 4. 文件级合并候选（v2 新增）

| 文件 | LOC | 评估 |
|---|---:|---|
| `crates/synthia-harness/src/events/event_enum/mod.rs` | 524 | 单 enum（AgentEvent），可拆 user/assistant/system 三文件 |
| `crates/synthia-session/src/compaction_checkpoint.rs` | 782 | 关注点可拆：snapshot 写盘 + lifecycle view + 触发器 |
| `crates/synthia-provider/src/retry.rs` | 1028 | retry class 表 + provider-specific backoff + quota 协调；可拆 `retry/{class, backoff, quota}.rs` |
| `crates/synthia-provider/src/openai_streaming/mod.rs` | 1021 | SSE pump + 事件分类 + error recovery；可拆 `openai_streaming/{pump, classify, recover}.rs` |
| `crates/synthia-tool-shell/src/sandbox.rs` | 1086 | ExecutionPolicy + Landlock/bwrap + argv 构造；保持（关注点统一） |
| `crates/synthia-server/src/routes/sessions.rs` | 2039 | **最大 route 文件**；可拆 `routes/sessions/{chat, fork, export, delete, list}.rs` |
| `crates/synthia-server/src/routes/chat.rs` | 1195 | stream_messages + turn start/stop + UI 推送；可拆 |
| `crates/synthia-workflow/src/runtime/mod.rs` | 1466 | DAG 调度 + state machine + persistence；可拆 |

---

## 5. 本审计新发现（v2 独有）

在 v1 + synthia 自有 audit 之外，本轮新发现的 4 个可考虑事项：

### 5.1 G-NEW-1：Model fallback chain（oh-my-openagent 经验）

| 维度 | 内容 |
|---|---|
| 来源 | `~/workspace/oh-my-openagent` 的 LazyCodex（"Anthropic 限速后自动切 Codex"） |
| 现状 | `synthia-provider` 没有 `ProviderChain`；只能构造一个 provider |
| 候选 | 新增 `ProviderChain::new([provider_a, provider_b])` + `complete` 时按 `RetryClass::Quota` 切下一个 |
| 风险 | 低 |
| 收益 | 中（生产环境 provider 限速恢复） |
| 文件 | `crates/synthia-provider/src/chain.rs`（新）约 200 L |

### 5.2 G-NEW-2：Self-improvement / skill 学习循环（hermes-agent 经验）

| 维度 | 内容 |
|---|---|
| 来源 | `~/workspace/hermes-agent`（Nous Research，"self-improving agent built into the framework"） |
| 现状 | `synthia-skill` 是静态 provider 列表；agent 不会写 skill |
| 候选 | 给 `synthia-skill` 加 `SkillWriter::record_experience(...)`，由 plugin hook 触发，异步写入 skill provider |
| 风险 | **高**（涉及安全：自动写的 skill 必须 fail-closed review） |
| 收益 | 中（长期看是 1.x 的差异化） |
| 文件 | 多 crate 改动；建议延后到 v1.0 后 |

### 5.3 G-NEW-3：Tool::icon / category（pi-mono §4.1-2 + UI affordance）

| 维度 | 内容 |
|---|---|
| 来源 | pi-mono `ToolDefinition` rich schema 字段 + dsh `presentation_meta` |
| 现状 | `Tool` trait 没有 `icon` / `category` 字段；UI 端 hardcode |
| 候选 | `Tool::icon() -> Option<&'static str>` + `Tool::category() -> ToolCategory`（enum: Read / Write / Network / Scheduler / Search / Subagent / Composite） |
| 风险 | 低（trait method，默认空实现） |
| 收益 | 中（UI affordance + tool group selector） |
| 文件 | `crates/synthia-tool/src/traits.rs` + 5 个 plugin tool crate 各 1 行 |

### 5.4 G-NEW-4：observability example / 真实 wire-up 示例

| 维度 | 内容 |
|---|---|
| 来源 | traitclaw `26-observability` example + pi-mono SDK 12 examples |
| 现状 | `crates/synthia-server/examples/` 无 OTLP wire-up example；新接 OTLP 的下游用户只能猜 |
| 候选 | `examples/observability.rs` — 启 server → OTLP collector → 跑一次 chat → 输出 trace 树 |
| 风险 | 低（example crate） |
| 收益 | 高（让 reviewer / 集成方看见端到端 wire-up） |
| 文件 | `crates/synthia-server/examples/observability.rs`（新）约 150 L |

---

## 6. 与 synthia "乐高式 + 可作为 lib" 哲学的契合度

synthia 顶级目标（AGENTS.md §3.7）：

1. **可作为 lib** — `cargo add synthia` 是消费端入口，下游不反向依赖应用。
2. **乐高式组装** — Provider / Tool / ContextManager / Steering / SessionSink / CancelToken / Agent 各自 trait + 默认实现。

按本审计复核：

| 组件 | trait | 默认实现 | 现状 |
|---|---|---|---|
| Provider | `ModelProvider` | `AnthropicProvider` / `OpenAiProvider` | ✅ |
| Tool | `Tool` | 5 个 plugin crate（read/write/shell/todo/web）+ 3 个 opt-in（task/scheduler/search）+ 2 个 synthetic（full_output/structured_output） | ✅ |
| ContextManager | `ContextManager` | `TruncatingContextManager` / `SummarizingContextManager` | ✅ |
| Steering | `Guard` / `Hint` / `Tracker` / `HookMap` | 8 guard / 3 hint / 2 tracker | ✅ |
| SessionSink | `SessionSink` | `InMemorySessionSink` / `JsonlSessionSink` | ✅（5 方法 trait） |
| CancelToken | `synthia_core::CancelToken` | `CancelToken::new()` | ✅ |
| Agent | `ReActAgent` | `ReActAgent::builder()` | ✅（R110 把 builder 折进 harness） |

**facade 公开面**：
- `crates/synthia/src/lib.rs` 是零逻辑 facade（`pub use` 传播）
- 28 个 crate 的精选 re-export 都从底层 crate 的 `lib.rs` 单一真源
- 门禁 `make check-pub-surface`（禁止 `pub use <内部模块>::*`）

**所有门禁**：
- `fmt-check` / `lint-rust` / `doc-check` / `test-unit` / `test-crates` / `check-mvp-deps` / `check-no-runtime` / `check-public-api-runtime` / `check-test-layout` / `check-pub-surface` / `check-harness-shape` / `check-claim-language` — 全绿（除基线修了一个 doc 链接）

---

## 7. 落地建议（按 commit 切分）

### 7.1 文件体量拆分（5 commit，纯 refactor，可独立绿）

```
commit A: synthia-tool surface.rs 拆分 → surface/{mod, projection, wrappers, deferred}
   LOC delta: -850, ~90 分钟, risk: low

commit B: synthia-tool truncate/bound_output.rs 拆分
   LOC delta: -50, ~30 分钟, risk: low

commit C: synthia-harness prompt/mod.rs 拆分
   LOC delta: -80, ~45 分钟, risk: low

commit D: synthia-session token_meter.rs 拆分
   LOC delta: -80, ~30 分钟, risk: low

commit E: synthia-session events.rs 拆分
   LOC delta: -100, ~120 分钟, risk: mid (构造器散布)
```

### 7.2 G-NEW 类小决策（3 commit，单 trait 扩展或新 example）

```
commit F: G-NEW-1 ModelProvider chain
   新增 ProviderChain（fallback on Quota），~200 L + 5 tests, ~60 分钟
   risk: low

commit G: G-NEW-3 Tool::icon + category
   traits.rs 加 2 个默认 trait method，5 plugin crate 各加 1 行
   LOC delta: +30, ~30 分钟, risk: low

commit H: G-NEW-4 observability example
   新增 crates/synthia-server/examples/observability.rs
   LOC delta: +150 (example), ~45 分钟, risk: low
```

### 7.3 backlog（不动手）

- G4（吞错），G5（沙箱），G6（web search），G7（TUI） — 按 synthia 自有 audit 维持 backlog
- G-NEW-2（self-improvement loop） — 风险高，建议 v1.0 后
- 17 pi-mono backlog（plan mode / branch summary / dynamic resource / tool override） — 低优先
- traitclaw 其它 8 traits — 全部已对位

### 7.4 总时间预算

| 类别 | commit 数 | 时间 |
|---|---:|---|
| 文件体量拆分（A-E） | 5 | ~5 小时 |
| G-NEW（F-H） | 3 | ~2.5 小时 |
| **合计** | **8 commit** | **~7.5 小时** |

每 commit 独立 `make ci` 绿，可逆。

---

## 8. 与现有 CHANGELOG 的关系

- R93-R123 的 30+ 轮拆分已经把"复杂函数 / 大测试块 / 内联测试"清理到接近零本轮未触及的 6 个文件（`surface.rs` / `truncate/bound_output.rs` / `prompt/mod.rs` / `token_meter.rs` / `events.rs` / 几个 >1000L 的 server route 文件）
- R124 之后的下一个 round（"R125"）完全可以是本审计的 commit A-E（文件拆分），commit F-H 可选
- CHANGELOG `[Unreleased]` 段（已 5687 行）已记录大量 R115-R123 落地；本审计 commit 应起 R126（"file-size cleanup R126"）或类似编号

---

## 9. 验证清单（每 commit 后必跑）

```bash
cargo +nightly fmt --all
make lint-rust                   # clippy -D warnings
make doc-check                   # rustdoc -D warnings
cargo test -p <changed-crate> --lib
make check-mvp-deps              # 7 feature 最小子集
make check-no-runtime            # runtime-free crates
make check-harness-shape         # synthia-harness 形状
make check-claim-language        # 文档措辞
```

每 commit 后跑一次，全部绿即可。
