# Synthia 当前架构深度分析（2026-07-12）

> 范围：`/home/crochee/workspace/synthia`（多 crate workspace，21 个 crate）。
> 视角：以 P1-P10 原则为镜，对 10 个核心子系统做 **现状 → 差距 → 借鉴路径** 的客观盘点。
> 目的：为 OpenSpec 后续变更提供 baseline 报告，非任务执行文档。

---

## 0. 总览与抽象栈

| 层 | 主要 crate | 核心抽象 | 状态 |
| --- | --- | --- | --- |
| Provider | `synthia-provider` | `Provider` trait、`CompletionRequest`/`Message`、`CachePolicy` | ✅ 稳定，对齐 opencode 语义 |
| Cache Mark | `synthia-cache-mark` | `CacheControlMark` / `CacheScope` / `CacheTtl` | ✅ 稳定 |
| Tool | `synthia-tool` + `synthia-tool-{bash,read,write,…}` | `Tool` trait + `ToolRegistry` + `ScopedToolRegistry` | ⚠️ 抽象成熟，但 bash 沙箱声明≠实保护 |
| Hook | `synthia-hook` | `AgentHook` trait + `FailPolicy` + `ToolAction` | ⚠️ 集成度低：main_loop 仅 fire_before/after_llm，未触发 on_before/after_tool |
| Skill | `synthia-skill` | `Skill` + `SkillRegistry` + `MatchingStrategy` | ⚠️ 三态机清晰，但隐式工具 `load_skill` 与 system prompt 注入仍待验证 |
| Plugin | `synthia-plugin` | `PluginManifest` + `HookRunner` + `McpProxy` | ✅ 声明/校验强，但 **双 hook 系统并存**（plugin hooks vs. agent hooks） |
| Agent Loop | `synthia-agent` | `StreamBuilder` / `main_loop` / `Step*` | ✅ 事件流 + JSONL 落盘 + 循环检测完整 |
| Session | `synthia-session` | `SessionStateMachine` + `Store` + `EventStore` | ✅ 状态机 + 持久化 + checkpoint 完善 |
| Context | `synthia-context` | `PrefixTracker` + `Compactor` + `Pruning` | ✅ 重要：prefix_hash 已有、pruning/compaction 分级存在 |
| Subagent | `synthia-agent::subagent` | `SubagentSessionFactory` + `ChildSessionHandle` | ⚠️ 工厂已就位但 **main_loop 未消费**（见 §6） |

仓库根 `Cargo.toml` 是 workspace 聚合；`synthia-server` 是 HTTP 入口；`synthia-tui` / `synthia-cli` 是两种 host；`synthia-telemetry`（`otel` feature）提供 OTel。

---

## 1. Tool 系统

### 1.1 现状

**Trait 抽象（`synthia-tool/src/traits.rs`）**
- `Tool` trait 7 个方法（`name` / `description` / `parameters` / `requires_permission` / `is_hidden` / `is_concurrency_safe` / `call`），加两个可选的 `call_with_sandbox`（`traits.rs:49-58`）、`call_with_progress`（`traits.rs:65-72`）。契约简洁。
- `is_concurrency_safe` 关键：纯工具可声明并发安全（`traits.rs:29-31`），由 `ToolOrchestrator` 通过 semaphore 限流（`registry/registration/registry.rs:269-281`，`max_concurrent=5`）。

**Registry**
- `ToolRegistry` 用 `RwLock<HashMap<String, ToolEntry>>`（`registry.rs:71`），`Clone` 走快照（`registry.rs:392-406`）—— 与 P10 文件即记忆一致：可序列化、可审计。
- 默认注册 7 个内置工具（`registry.rs:109-121`）：`ReadTool` / `WriteTool` / `GlobTool` / `GrepTool` / `MultiEditTool` / `ApplyPatchTool` / `WebFetchTool`。
- `run_with_context` 是单一调度入口：先做权限检查（`registry.rs:174-225`），再分桶到 `execute_tools`（`registry.rs:236-248`）；每个输入槽必有输出（`registry.rs:250-253`），符合 P2 append-only 精神。
- 关键：hidden 工具在分发时被过滤（`registry.rs:165-168`）—— 隐式工具如 `load_skill` 不会暴露给 LLM 的 tool_choice 枚举。

**内置工具族**
- `synthia-tool-bash`：`BashTool` + `MonitorTool` + `CommandManager`（子进程池化）。
- `CommandBlacklist` 在 `command_blacklist.rs` 是 **字符串子串匹配**（`command_blacklist.rs:39-68`），模块注释自己写明 5 类 bypass 都不拦（`command_blacklist.rs:1-27`）。`DEFAULT_MAX_OUTPUT_BYTES=64KB`（`command_blacklist.rs:35`），`truncate_output` 走 UTF-8 safe（`command_blacklist.rs:178-187`）。
- `validate_path` 走 `clean_path` 拆组件 + `starts_with(workspace_root)`，无 canonicalize 依赖（`command_blacklist.rs:106-152`）。

**Orchestrator / 执行**
- `ToolOrchestrator`（`tool_executor` 模块）支持并发、超时、并发安全短路；`sandbox_manager` 已存在但 `main_loop` 中 `_sandbox_manager` 字段被丢弃（`main_loop.rs:157`）。

### 1.2 差距

| 现状 | 差距 | 借鉴路径 |
| --- | --- | --- |
| 沙箱声明接口 `call_with_sandbox` 已设计但只有 bash 覆盖（其他 tool 默认委托 `call`） | 大部分 Tool 没有真正隔离能力 | 集成 Landlock / bubblewrap / firejail 子进程沙箱；非 bash tool 应至少提供 `workspace_root` 约束 |
| `CommandBlacklist` 是注释级警示——只是"防御性辅助" | 实际不是 OS 级 sandbox | 把 blacklist 重命名为 `defense_in_depth_hint`；强制要求 sandbox 来自 `synthia-guardian::sandbox` 或 OS-level |
| `is_concurrency_safe` 默认 `false`（保守） | Read 等纯工具会浪费并发机会 | 评估内置工具并发安全矩阵；显式打开纯 read/search 工具标志 |
| `ToolRegistry` 没有 `unregister` 的 trait 钩子（trait 在 `registry_trait.rs`） | 插件热更新时残留 | 在 `Registry` trait 上加 `unregister_observed_by()`，触发 P9 事件 |
| `max_concurrent=5` 是 hardcoded 默认 | 无配置驱动 | 让 `AgentRunConfig` 注入 `tool_concurrency`，呼应 P4 可降级 |

### 1.3 哪些抽象应该被重构成 Tool

| 现有抽象 | 位置 | 重构为 Tool 的理由 |
| --- | --- | --- |
| `synthia-context::compact_context_tool` | `compact_context_tool.rs` | 已经是 Tool，但 `main_loop` 把它当 facade 走 P3 懒加载（`main_loop.rs:555-561`，`770-795`）；考虑统一为同一 Tool 接口 |
| `synthia-skill::implicit_tools::load_skill` | `synthia-skill/src/implicit_tools/` | LLM 通过此 Tool 触发 skill 加载；应与正常 tool 走同一注册 + hidden 路径 |
| `synthia-agent::subagent::AgentTool` | 推测在 `agent_tools.rs` | 应直接复用 `ToolRegistry`，避免"Tool 之外的 Tool"双轨 |
| `synthia-guardian::SELF_REFLECT_TOOL_NAME` | `synthia-guardian` | 内部用 `c.name ==` 字面量比较（`main_loop.rs:543-546`），应让 Tool 自报家门 |
| `MonitorTool` | `synthia-tool-bash/src/monitor.rs:7-44` | 已经按 static 风格实现（非 `Tool` trait），应迁移 |

---

## 2. Hook 系统

### 2.1 现状

**Trait（`synthia-hook/src/traits.rs`）**
- `AgentHook` 7 个生命周期方法（`traits.rs:99-153`）：
  - `on_error` / `on_before_llm` / `on_after_llm` / `on_before_tool` / `on_after_tool` / `on_iteration_end` / `on_complete`。
- `FailPolicy::{FailOpen, FailClosed}`（`traits.rs:79-84`）默认 `FailOpen`（⚠️ 与项目硬约束"permission 必须 fail-closed"不一致——permission 层做了 fail-closed，但 hook 默认是 fail-open）。
- `ToolAction::{Proceed, Skip, Modify, PendingConfirm}`（`traits.rs:86-96`）—— 颗粒度足够：可拦截、可改 input、可申请确认。
- `AgentContext`（`traits.rs:50-77`）持有 messages / pending_tool_calls / metadata，是 in-place 可变上下文（`on_before_llm` 拿 `&mut`）。

**Registry**（推测在 `synthia-hook/src/registry/`）
- 未在本轮详细阅读但 `HookRegistry` 已存在，被 `main_loop` 通过 `steps.hooks` 持有（`main_loop.rs:439`）。

### 2.2 差距

| 现状 | 差距 | 借鉴路径 |
| --- | --- | --- |
| `main_loop` 只 fire `on_before_llm`（`main_loop.rs:439-441`）+ `on_after_llm`（`main_loop.rs:588-590`） | 6 个 hook 中 4 个（`on_before_tool` / `on_after_tool` / `on_iteration_end` / `on_error` / `on_complete`）实际未被触发 | 在 `StepToolExecute`（`tool_executor`）中嵌入 `on_before_tool` / `on_after_tool`；在 loop 末尾 `on_iteration_end`；在错误分支 `on_error` |
| `FailPolicy` 默认 `FailOpen` | 任何 hook 异常会让工具继续执行 | 默认改为 `FailClosed`（对照 hard-constraint 列表中 permission 的方向），显式 OptIn 才能 FailOpen |
| `ToolAction::PendingConfirm { blocking }` 字段在 agent 层未实现 | "软中断"路径不存在 | 在 `ToolRegistry::run_with_context`（`registry.rs:191-225`）的权限检查位之前插入 `on_before_tool` 拦截 |
| `AgentContext::messages: Vec<Message>` 是 owned 拷贝 | P2 append-only 原则下，hook 改 messages 后 hash 会变；当前没有 `messages_hash_after_hook` 记录 | 在 `on_after_llm` 之后重算 `canonical_messages_prefix_bytes` 并加入 `PrefixTracker` |
| 没有 hook 链式优先级 | 多 hook 冲突时无裁决 | 引入 `priority: i32` 字段，仿照 plugin hooks（`hook_runner::load` 注释 `types.rs:8-31`） |

### 2.3 哪些抽象应该被重构成 Tool

| 现有抽象 | 位置 | 路径 |
| --- | --- | --- |
| `HookRunner` 的 Command hook（外部子进程） | `synthia-plugin/src/hook_runner/execute.rs` | 把"外部子进程 hook"统一为 `Tool` 接口，副作用是 hook 也是 token-budgeted |
| Plugin 的 `pre-task` / `post-task` hook | `synthia-plugin/src/manifest.rs:38-40` | 走 `AgentHook` 而不是 `HookRunner`，消除双 hook 系统 |

---

## 3. Plugin 系统

### 3.1 现状

**Manifest（`synthia-plugin/src/manifest.rs`）**
- `PluginManifest` 5 字段（`manifest.rs:48-69`）：`name`（kebab-case，`manifest.rs:11-18`）+ `version`（semver，`manifest.rs:21-23`）+ `description` + `author` + 可选 `hooks` / `mcpServers`。
- `validate()` 强制 name/version（`manifest.rs:117-129`）—— 严格契约。
- `PluginError` 9 种（`manifest.rs:73-105`）—— 错误分类细。

**HookRunner（`synthia-plugin/src/hook_runner/mod.rs`）**
- 子模块清晰：`core` / `execute` / `fire` / `load` / `types`（`mod.rs:49-58`）。
- `fire()` 按 priority 排序、regex 匹配 target/event、`execute_hook` 区分 Command vs Prompt、`tokio::time::timeout` 兜底（`mod.rs:33-47` 注释）。
- `SharedHookRunner`（`mod.rs:60`）是 thread-safe 包装。

**McpProxy**
- 目录 `mcp_proxy/`（`crates/synthia-plugin/src/mcp_proxy/`，未详细展开），承担 MCP server 生命周期。

**Registry**（`crates/synthia-plugin/src/registry/`）
- `handle.rs` / `store.rs` / `types.rs` / `tests.rs` —— 4 文件结构，handle 是核心句柄。

### 3.2 差距

| 现状 | 差距 | 借鉴路径 |
| --- | --- | --- |
| **双 hook 系统并存**：`AgentHook`（`synthia-hook`）vs `HookRunner`（`synthia-plugin`） | 同一概念两套抽象，开发者选择困难 | 把 plugin hooks 全部转化为 `AgentHook` impl（`impl AgentHook for PluginHookAdapter`），统一 lifecycle |
| Manifest 接受任意 `hooks: serde_json::Value`（`manifest.rs:64`） | 没有 schema 验证，运行时才报错 | 用 `schemars` 生成 JSON Schema + 启动期校验 |
| `mcp_proxy` 与 `synthia-server` 的 MCP 集成路径不明 | 可能重复实现 | 在 `mcp_proxy` 上做单一入口：把 `McpProxy` 设计为 `ToolRegistry::register` 的"工具源" |
| Plugin 加载无版本兼容性检查 | 老 plugin 在新 core 上会随机失败 | 在 `PluginManifest` 加 `min_core_version` 字段，semver 范围匹配 |
| 无 plugin 卸载（hot-unload）的 hook 通知 | 子进程/CPU 资源残留 | `registry/handle.rs` 加上 `Drop` 时 `HookRunner::unregister` |

### 3.3 哪些抽象应该被重构成 Tool

| 现有抽象 | 位置 | 路径 |
| --- | --- | --- |
| 每个 MCP server | `mcp_proxy` | 显式实例化为 `Tool`：`McpTool { server: Arc<McpProxy>, name: String }`，统一走 `ToolRegistry` |
| Plugin 自己的 CLI 入口 | `manifest.hooks` 当前是 dict，能力受限 | 改 `hooks: Vec<HookSpec>` 并加 `kind: Tool \| Agent \| Subscription` |

---

## 4. Skill 系统

### 4.1 现状

**匹配策略（`synthia-skill/src/matcher/strategy.rs`）**
- `MatchingStrategy::{Keyword, Embedding, Hybrid}`（`strategy.rs:24-45`），默认 `Keyword`（`strategy.rs:27-28`）—— P3 原则契合：选最便宜的。
- `Embedding` 实际是 placeholder（`strategy.rs:31-34`），`Hybrid { keyword_weight, embedding_weight }` 真实生效。

**Registry（`synthia-skill/src/registry/`）**
- `SkillRegistry`（`mod.rs:26`）持有 `skills: RwLock<HashMap>`、`active_skills`、token counter。
- `activate_skill`（`registry/lifecycle/activation.rs:25-81`）：
  1. 拓扑排序解析依赖（`resolve_dependencies`，`activation.rs:29-30`）。
  2. `check_conflicts`（`activation.rs:33`）—— P3 隐式检查。
  3. 按顺序激活：Level0→Level1，`session_token_counter.fetch_add`（`activation.rs:60-65`）。
- `deactivate_skill`（`activation.rs:83-96`）对称递减。

**Loader / Watcher / Installer / Usage**
- 4 个并列子目录 `loader.rs` / `watcher/` / `installer/` / `usage.rs`，外加 `bm25` / `embedding` / `implicit_tools` / `builtin` / `matcher` / `registry` / `tool_registry.rs` —— 高度模块化。

### 4.2 差距

| 现状 | 差距 | 借鉴路径 |
| --- | --- | --- |
| `MatchingStrategy::Embedding` 是 placeholder（`strategy.rs:31-34`） | 不能用真 embedding | 接 `synthia-embedding` 子系统（待盘点） |
| Skill 加载走 system prompt 注入路径，但**没有看到 system_prompt 的 byte-level lock** | 任意 prompt 改动会破坏 P1 | 把 `SkillRegistry::active_skills` 的快照（按 name 排序）作为 prefix hash 的第四项，对齐 `PrefixTracker` |
| `session_token_counter` 是 `AtomicI64`-ish（`activation.rs:64-65`） | 没有按 event 持久化，P9 不可观测 | 把 `fetch_add` 包成 `tracing::info!` + JSONL `skill_activated` 事件 |
| `implicit_tools::load_skill` 未在主循环显式注册到 `ToolRegistry` | LLM 看不到此 Tool | 在 `component_assembly.rs` 显式 `registry.register(LoadSkillTool::new(registry.clone()))` |
| `usage.rs` 追踪调用次数 | 没有"未使用自动降级" | 周期任务把长期未用的 skill 退到 `Level0` |

### 4.3 哪些抽象应该被重构成 Tool

| 现有抽象 | 位置 | 路径 |
| --- | --- | --- |
| `load_skill` 隐式工具 | `synthia-skill/src/implicit_tools/` | 走 `Tool` trait，注册时 `is_hidden=true` |
| `usage` tracker | `synthia-skill/src/usage.rs` | 改为 `Tool`："query skill usage stats"，提供 self-reflection 接口 |
| `watcher` 文件变更监听 | `synthia-skill/src/watcher/` | 保持独立（不是 LLM 可见 Tool） |

---

## 5. Agent Loop

### 5.1 现状

**入口（`synthia-agent/src/stream_builder/builder/run/main_loop.rs`）**
- `StreamBuilder::run_with_steps`（`main_loop.rs:107-123`）是核心 `stream!` 块，参数 7 个：run_config + steps + initial_state + prefix_tracker + on_prefix_event + on_usage + system_snapshot。
- 主循环 `while !ctx.should_stop_with_timeout(...)`（`main_loop.rs:245-248`），每次迭代：
  1. **drain steering**（`main_loop.rs:250-257`）—— P7 软中断。
  2. **background subagent check**（`main_loop.rs:261-276`）—— 注入 `<task>` XML。
  3. **iteration++ + turn_id 分配**（`main_loop.rs:278-279`）。
  4. **cancellation check**（`main_loop.rs:310-377`）—— 失败 in-flight tool（`fail_interrupted_tools`，`main_loop.rs:319`）保证 P8 + P5。
  5. **do_compact_step**（`main_loop.rs:381-428`）—— 渐进降级。
  6. **build_tool_definitions**（`main_loop.rs:430-434`）。
  7. **prefix snapshot**（`main_loop.rs:448-457`）—— 调用 `PrefixTracker::record_pre`，P1 关键。
  8. **LLM sample cascade**（`main_loop.rs:459-469`）。
  9. **outcome dispatch**（`main_loop.rs:471-853`）：`Continue` / `Terminate` / `Done` 三分支。
  10. **end-of-session reflect**（`main_loop.rs:883-891`）。

**关键设计点**
- 三个 outcome 都显式 `emit_turn_event(TURN_FAILED)`（`main_loop.rs:480-488, 496-505, 670-680`）—— P8 JSONL 不丢。
- `maybe_auto_trigger_self_reflect`（`main_loop.rs:906-935`）80% 阈值（注释 `main_loop.rs:961`）。
- `maybe_auto_trigger_compact_context`（`main_loop.rs:950-983`）同阈值 + dedup `llm_compact_called_this_iter`。
- `format_background_task_notification`（`main_loop.rs:82-99`）把子任务结果包装成结构化 XML —— P5 末尾复述。

### 5.2 差距

| 现状 | 差距 | 借鉴路径 |
| --- | --- | --- |
| `subagent_session_factory` 字段被命名为 `_subagent_session_factory`（`main_loop.rs:153`）—— **没有解构使用** | 工厂已经注入但 main_loop 不消费，subagent 不能从这里启动 | 把 `_` 去掉，定义 `subagent_tool: SubagentTool` 并注册到 `ToolRegistry` |
| `sandbox_manager` 同样被丢弃（`main_loop.rs:157`） | `Tool` 的 `call_with_sandbox` 永远拿不到真沙箱 | 注入 `Arc<SandboxManager>` 到 `StepToolExecute` |
| `extension_manager` 同样丢弃（`main_loop.rs:161`） | extension 系统形同虚设 | 同上 |
| `fork_policy` 也被丢弃（`main_loop.rs:140`） | 会话分叉未启用 | 在恢复时检查 `fork_policy`，支持 read-only / writable fork |
| `approval_service` 同样丢弃（`main_loop.rs:158`） | `ToolAction::PendingConfirm` 永远走不到 | 串到 `StepToolExecute::on_before_tool` |
| `guardian_coordinator` 同样丢弃（`main_loop.rs:160`） | 循环检测结果不能反向干预 | 把 `loop_reason` 注入到 `on_before_tool` 决策 |
| `model_router` 丢弃（`main_loop.rs:128`） | 无动态模型切换 | 在 `sample_llm_and_cascade` 之前路由 |

> **核心观察**：`AgentRunConfig` 11 个字段，其中 9 个在 `main_loop` 入口被解构为 `_`。这是 §10 关键 gap。

### 5.3 哪些抽象应该被重构成 Tool

| 现有抽象 | 路径 |
| --- | --- |
| `SubagentSessionFactory` | `AgentTool: Tool` |
| `ExtensionManager` | `ExtensionTool: Tool`（按需延迟注册） |
| `ForkPolicy` | `ForkTool: Tool`（会话分叉） |

---

## 6. Extension / Subagent

### 6.1 现状

**Factory（`synthia-agent/src/subagent/factory.rs`）**
- `SubagentSessionFactory` trait（`factory.rs:48-103`）：
  - `create_child(user_id, parent_session_id, maybe_id, parent_depth)` —— 创建子会话。
  - `run_child(...)` 默认实现回退到 `create_child` + 占位 `AgentResult`，要求 server-side override。
- `ChildSessionHandle { session_id, user_id, parent_event_sender }`（`factory.rs:24-29`）—— parent_event_sender 用于把子事件镜像为 `AgentEvent::SubagentEvent`。
- `parent_depth` 参数（`factory.rs:54-56`）—— 防止递归爆炸。
- `truncate_summary(s, max_chars)`（`factory.rs:117-135`）—— UTF-8 safe，按 char 截 500 字符 + `… [truncated]` 指示符。

**Guardian Bridge / Permission（`synthia-agent/src/subagent/guardian_bridge.rs`、`permission.rs`、`config.rs`）**
- 4 个文件结构清晰。

**Mod（`synthia-agent/src/subagent/mod.rs`）**
- 暴露 `factory` / `config` / `permission` / `guardian_bridge`。

### 6.2 差距

| 现状 | 差距 | 借鉴路径 |
| --- | --- | --- |
| `main_loop` 完全不消费 `subagent_session_factory`（`main_loop.rs:153`） | subagent 路径未触发 | 显式 `let subagent_tool = SubagentTool::new(subagent_session_factory.clone())` 并注册 |
| `run_child` 默认实现返回 `"run_child not implemented"`（`factory.rs:93-101`） | 静默错误 | trait 上加 `unimplemented!()` 风格的 `compile_error!` 或默认 `panic!` |
| `ChildSessionHandle::parent_event_sender` 是 `Option<...>` | 何时为 None 文档不清 | 加 `#[must_use]` 注释和典型路径图 |
| `parent_depth` 传到了 factory 但**没有看到深度上限硬编码** | 递归死循环可能 | 在 `AgentRunConfig` 上加 `max_subagent_depth: usize` 默认 3 |
| `truncate_summary` 500 字符是 hardcoded | 不灵活 | 走 `subagent-background-mode` spec 的 config |

### 6.3 哪些抽象应该被重构成 Tool

- `SubagentSessionFactory` 直接包成 `SubagentTool: Tool`，`call(input)` 把 input 解析为 `prompt` + `parent_depth`，调 `run_child`，把 `truncate_summary` 后的结果作为 tool_result。

---

## 7. Provider / Cache Policy

### 7.1 现状

**CachePolicy（`synthia-provider/src/cache_policy.rs`）**
- `CachePolicy` 4 字段（`cache_policy.rs:51-58`）：`tools` / `system` / `messages` (`MessageCacheStrategy::{None, LatestUserMessage}`) / `ttl_seconds`。
- Default 对齐 opencode `AUTO`（`cache_policy.rs:60-69`）。
- `apply_cache_policy(request, policy)`（`cache_policy.rs:105-121`）**幂等**（覆盖式非追加，`cache_policy.rs:18-20` 注释）。
- `CachePolicyApplier`（`cache_policy.rs:137-201`）基于 `Arc::ptr_eq` 短路：tools/messages Arc 同引用 → 返回 true 不重做（`cache_policy.rs:170-187`）。**这是 P1 前缀一致性的核心**。

**TTL 映射（`cache_policy.rs:77-83`）**
- `None` 或 ≤300s → `Ephemeral`；>300s → `Extended`。

**Scope 注释（`cache_policy.rs:30`）**
- "system 标记 defer 到 provider 的 `transform_request`" —— 因为 system 嵌在 `messages: Vec<Message>` 里。

### 7.2 差距

| 现状 | 差距 | 借鉴路径 |
| --- | --- | --- |
| `CacheScope::default()` 拿不到 user_id（`cache_policy.rs:88-90`） | 跨用户 cache 污染风险（项目硬约束明文列出） | 在 `CachePolicyApplier::apply` 上加 `user_id: &str` 参数；`mark_from_policy` 接受 scope |
| 短路逻辑只覆盖 `tools` + `messages`（`cache_policy.rs:170-179`） | 若 `model` 字段变了，cache 也会失效但不会被短路检测 | 把 `request.model` 也纳入 `previous_*` |
| 没有 provider-specific transform 的统一入口 | Anthropic / OpenAI 各自实现 `transform_request` | 在 `Provider` trait 上加 `transform_request_with_cache` 钩子 |
| `ttl_from_policy` 阈值 300s 是 hardcoded | 不同 provider TTL 上限不同 | 走 `Provider::cache_ttl_limit()` |
| `apply_cache_policy` 改 last_tool，但 `apply_policy_all_disabled_is_noop` 测试说"全禁用 = 跳过" | 实际测试通过 | OK，但需保证 provider 层尊重 `policy.tools=false` |

### 7.3 哪些抽象应该被重构成 Tool

- `CachePolicy` 已经是 provider-neutral 的 enum 结构，不重构成 Tool；但 `CachePolicyApplier` 的"短路信号"可以暴露成 `Tool`：`CacheInspectTool: Tool`，让 LLM 主动查询当前 cache 状态（用于 OTel-driven prompt 优化）。

---

## 8. Session 持久化

### 8.1 现状

**StateMachine（`synthia-session/src/state_machine/machine.rs`）**
- `SessionStateMachine`（`machine.rs:18-22`）持有 `current_state` + `session_store: Store`。
- `transition_to(target, session)`（`machine.rs:53-93`）：校验 `is_valid_transition` → 更新状态 → `session_store.save_metadata` → 触发 `on_state_enter` side effects → 返回 `StateEnterEffect`。
- `on_state_enter`（`machine.rs:98-132`）只 logging；async effects（如 `StartApprovalTimeout`）通过 `StateEnterEffect` 提示。
- `StateEnterEffect`（推测在 `transitions/`）枚举 `StartApprovalTimeout` / `CancelApprovalTimeout` / `None`（测试见 `machine.rs:251-303`）。

**Submodules**
- `state_machine/machine.rs` + `state_machine/transitions/` + `state_machine/tests.rs` 三层。

**Store + EventStore + Manager + Token Budget（`synthia-session/src/`）**
- `store.rs` / `event_log/` 隐含 / `manager/` / `token_budget.rs` —— 全栈持久化。

### 8.2 差距

| 现状 | 差距 | 借鉴路径 |
| --- | --- | --- |
| `transition_to` 每次都 `save_metadata`（`machine.rs:84-86`） | 高频切换时 IO 放大 | 加 dirty 标记，N 次或 T 秒批量 flush |
| `is_valid_transition` 集中校验但**未在 main_loop 中看到显式调用** | 状态可能漂移 | 把状态机作为 `Step*` 的输入约束，违反就警告 |
| `WaitingForApproval` 的 `StartApprovalTimeout` 提示在 `on_state_enter` 中**只是 logging**（`machine.rs:106-110`） | 实际定时器未启动 | 接入 `tokio::time::sleep` 异步定时器 |
| 持久化路径无 `user_id` namespace 显式传播（虽然 `Store::load_metadata` 接受 user_id） | 测试用 `SERVER_DEFAULT_USER_ID`（`machine.rs:161`） | 强制 user_id 来自认证层，不允许 default fallback |
| `EventStore` 跟 `Store` 是两个东西 | 概念分散 | 合并为 `Store { metadata, events, checkpoints }` |

### 8.3 哪些抽象应该被重构成 Tool

- `SessionStateMachine::current_state()` 可以变成 `SessionInspectTool: Tool`，让 LLM 知道当前在哪一步（用于 P5 末尾复述）。
- `checkpoint` 模块（`synthia-agent/src/checkpoint/`）目前是独立路径，应统一为"save/load session" Tool。

---

## 9. P1-P10 原则满足度自评

| 原则 | 满足度 | 关键证据 | 主要 gap |
| --- | --- | --- | --- |
| **P1 前缀一致性** | 80% | `PrefixTracker::compute_hash_bytes` 三段拼接（`tracker.rs:77-87`），`canonical_messages_prefix_bytes` 尊重 `tool_result_cleared_at`（`tracker.rs:108-114`），`CachePolicyApplier` Arc ptr_eq 短路（`cache_policy.rs:165-200`） | (1) skill snapshot 没纳入 hash；(2) `request.model` 未纳入短路 |
| **P2 Append-Only** | 70% | `Message` 上 `tool_result_cleared_at: Option<Instant>` 幂等 marker（`PrefixTracker` 注释 `tracker.rs:101-114`），`truncate` 走 `cleanup_tool_output_store_async`（`main_loop.rs:178`） | (1) main_loop 直接 `ctx.messages.push(synthetic_msg)`（`main_loop.rs:269`）绕过统一 API；(2) `previous_summary` 截断到 4000 char 是 hardcoded |
| **P3 按需加载** | 75% | `MatchingStrategy::Keyword` 默认（`strategy.rs:27-28`），`Skill` Level0/Level1/Level2/Level3，system prompt 注入遵循 skill 激活 | (1) `load_skill` Tool 隐式注册缺失；(2) skill 内容是 JSON 注入，未走 lazy 字节追加 |
| **P4 渐进降级** | 60% | `compaction/{level1,level2,level3,orchestrator}.rs` 三阶段存在（`compactor.rs:8-19`），`pruning/{stages,engine,classify}.rs` 引擎就位 | (1) `pruning/stages.rs` 三阶段实际触发条件未在 main_loop 串起来；(2) Stage 2/3 触发阈值无统一 dashboard |
| **P5 末尾复述** | 65% | `format_background_task_notification` 注入末尾（`main_loop.rs:82-99`），`ctx.messages.push`（`main_loop.rs:269`） | (1) 无 `todo.md` 风格工作状态文件读取路径；(2) 失败工具结果只走 `add_tool_result`（`main_loop.rs:326-331`），未强制尾部回放 |
| **P6 不信任 LLM** | 70% | `LoopDetectorSet` 在 main_loop 创建（`main_loop.rs:230`），`check_doom_loop`（`main_loop.rs:658-694`），`is_command_blacklisted` 防御 | (1) `FailPolicy` 默认 `FailOpen`（`traits.rs:81-82`）—— 与项目硬约束相悖；(2) 4 类循环检测器需要逐个核对（见 §10） |
| **P7 可中断性** | 75% | `drain_steering`（`main_loop.rs:250-257`），`fail_interrupted_tools`（`main_loop.rs:319-348`），`cancel_token` 全链路 | (1) 没有 AbortSignal 全链路证据；(2) Session Reset 路径未实现 |
| **P8 不丢信息** | 85% | JSONL 事件落盘 `append_agent_event`（`main_loop.rs:46-77`），TURN_* 事件完整（`TURN_STARTED` / `TURN_COMPLETED` / `TURN_FAILED` / `SESSION_ENDED`） | (1) pruned 内容的 retrieval 路径未实现（`memory_search` 缺失）；(2) `pruning/classify` 决定是否可恢复没串到事件流 |
| **P9 可观测性** | 80% | `PrefixStabilityEvent`（`tracker.rs:236-242`），`CompactionAnalyticsAttempt`（`main_loop.rs:783-790`），`stability_ratio` rolling window（`tracker.rs:200-213`） | (1) `pruning_stage_distribution` 指标未聚合；(2) 缺少 `prefix_stability_ratio` 实时 dashboard |
| **P10 文件即记忆** | 90% | `cleanup_tool_output_store_async`（`main_loop.rs:178`），session/store 走文件，event_log 走 JSONL | (1) `memory_search` 不是文件 grep；(2) 笔记 / todo 工具未集成 |

> **P1 前缀一致性的"未纳入"**：
> 1. `system_snapshot: Vec<u8>`（`main_loop.rs:122`）虽然被 prefix_tracker 使用，但 skill snapshot 不在 hash 中（`compute_hash_bytes` 三参数没含 skill）。
> 2. `request.model` 在 `CachePolicyApplier` 短路里未比较（`cache_policy.rs:170-179`）。

---

## 10. 关键 Gap 列表

按"严重度 × 修复成本"排序（高严重度 = 影响 P 原则，低修复成本 = < 1 PR）。

### 10.1 严重（影响核心原则）

| # | Gap | 关联原则 | 修复路径 |
| --- | --- | --- | --- |
| G1 | **`AgentRunConfig` 11 个字段中 9 个在 main_loop 入口被丢弃**（`main_loop.rs:124-162`），包括 `subagent_session_factory` / `sandbox_manager` / `extension_manager` / `approval_service` / `guardian_coordinator` / `model_router` / `fork_policy` 等 | P3, P6, P7 | 逐个串到 `Step*`，改名去掉 `_` 前缀 |
| G2 | **`FailPolicy` 默认 `FailOpen`（`synthia-hook/src/traits.rs:81-82`）与项目硬约束"permission 必须 fail-closed"相悖** | P6 | 改为 `FailClosed` 默认，提供 `FailOpen` 显式 OptIn |
| G3 | **`CachePolicyApplier` 未含 user_id namespace 短路**（`cache_policy.rs:137-201`），与项目硬约束"cache control 必须含 user_id namespace"不符 | P1 | `apply` 加 `user_id: &str` 参数 |
| G4 | **`AgentHook` 的 `on_before_tool` / `on_after_tool` / `on_error` / `on_iteration_end` / `on_complete` 在 main_loop 未触发**（`main_loop.rs` 只 fire 两个 llm hook） | P6, P8 | 在 `StepToolExecute` 嵌入 4 个工具 hook |
| G5 | **Skill snapshot 不在 `PrefixTracker::compute_hash_bytes` 中**（`tracker.rs:77-87`）—— skill 激活改变 system prompt 但 hash 不变 | P1 | 第四参数 `skill_snapshot: &[u8]` 参与 hash |
| G6 | **双 hook 系统并存**：`AgentHook`（`synthia-hook`）vs `HookRunner`（`synthia-plugin`） | P9, P10 | 把 plugin hooks 全部走 `AgentHook` trait |

### 10.2 中（影响体验/可维护性）

| # | Gap | 修复路径 |
| --- | --- | --- |
| G7 | `is_concurrency_safe` 默认 `false`（`tool/traits.rs:29-31`）—— Read 等纯工具串行 | 显式标记并发安全工具 |
| G8 | `CommandBlacklist` 注释自承 5 类 bypass 不拦（`command_blacklist.rs:1-27`），但函数名暗示是 sandbox | 重命名为 `DefensivePatternHint` |
| G9 | `MatchingStrategy::Embedding` 是 placeholder（`skill/matcher/strategy.rs:31-34`） | 实现真 embedding 后端 |
| G10 | `ToolRegistry::Clone` 走快照（`tool/registry/registration/registry.rs:392-406`）—— 每次 Clone 复制整个 HashMap | 用 `Arc<RwLock<HashMap>>` 共享 |
| G11 | `previous_summary` 截断 4000 char 是 hardcoded（项目硬约束明文） | 抽成 `SummaryConfig` |
| G12 | `SessionStateMachine::transition_to` 每次都 `save_metadata`（`machine.rs:84-86`） | 批量 flush + dirty flag |
| G13 | `WaitingForApproval` 的 `StartApprovalTimeout` 仅 logging（`machine.rs:106-110`） | 启动真实定时器 |
| G14 | `pruning/stages.rs` 三阶段未在 main_loop 串联 | 接到 `do_compact_step` |

### 10.3 低（可选优化）

| # | Gap | 修复路径 |
| --- | --- | --- |
| G15 | `PluginManifest.hooks: serde_json::Value`（`plugin/manifest.rs:64`）无 schema | `schemars` 验证 |
| G16 | `SubagentFactory::run_child` 默认返回 `"not implemented"`（`factory.rs:93-101`） | 改为 `compile_error!` 或 `unimplemented!()` |
| G17 | `MonitorTool`（`tool-bash/src/monitor.rs:7-44`）按 static 风格实现，不实现 `Tool` trait | 迁移到 `Tool` |
| G18 | `truncate_summary` 500 字符 hardcoded（`factory.rs:117-135`） | 走 config |
| G19 | `parent_depth` 传到了 factory 但**无深度上限硬编码** | 在 `AgentRunConfig` 加 `max_subagent_depth` |
| G20 | `usage.rs` 追踪调用次数但无自动降级 | 周期任务把长期不用的 skill 退到 `Level0` |

### 10.4 哪些抽象应该被重构成 Tool（汇总）

| 抽象 | 来源 | 目标 | 理由 |
| --- | --- | --- | --- |
| `SubagentSessionFactory` | `synthia-agent/src/subagent/factory.rs:48` | `SubagentTool: Tool` | 当前 main_loop 丢弃工厂；包成 Tool 后可发现、可限流、可 budget |
| `ExtensionManager` | `synthia-agent`（推测） | `ExtensionTool: Tool` | 同上 |
| `ForkPolicy` | `synthia-agent`（推测） | `ForkTool: Tool` | 提供会话分叉能力 |
| `load_skill` 隐式工具 | `synthia-skill/src/implicit_tools/` | 走 `Tool` + `is_hidden=true` | 统一注册路径 |
| MCP servers | `synthia-plugin/src/mcp_proxy/` | `McpTool` 包装 | 走 `ToolRegistry` 单一调度 |
| `usage` tracker | `synthia-skill/src/usage.rs` | `SkillUsageTool: Tool` | self-reflection 入口 |
| `SessionStateMachine::current_state` | `synthia-session/src/state_machine/machine.rs:39` | `SessionInspectTool: Tool` | 末尾复述可发现 |
| `compact_context` facade | `synthia-context/src/compact_context_tool.rs` | 已 Tool 化 | 保持 |
| `self_reflect` (LLM-driven) | `synthia-guardian` | 已 Tool 化 | 保持 |

---

## 11. 总结与建议

**Synthia 当前架构（2026-07-12 快照）**：

1. **抽象已成型**：Tool / Hook / Skill / Plugin / Cache / Session / PrefixTracker 6 大核心抽象都有清晰 trait + registry + lifecycle。
2. **持久化与可观测性扎实**：JSONL 事件 + Span + OTel + PrefixStabilityEvent + CompactionAnalyticsAttempt 全栈。
3. **P1-P10 原则中 P1 / P8 / P9 / P10 满足度高**，P3 / P4 / P5 / P6 有显著 gap。
4. **最关键的结构性问题**：`AgentRunConfig` 11 字段中 9 个被丢弃（§10.1 G1），导致 6 个子系统（subagent / sandbox / extension / approval / guardian / fork）处于"声明存在但未启用"状态。
5. **双 hook 系统**（G6）需要统一为 `AgentHook`，否则 plugin 与 agent 各自演化会出分裂。
6. **CachePolicyApplier** 缺 user_id namespace 是已知硬约束的违例（G3），必须修复。

**下一步 OpenSpec 候选**：
- **变更 1**：修复 G1（`AgentRunConfig` 字段串接），逐个 crate 拆分小 PR。
- **变更 2**：统一 hook 系统（G2 + G4 + G6）—— 涉及 `synthia-hook` / `synthia-plugin` 接口重整。
- **变更 3**：Skill snapshot 进入 prefix hash（G5）—— 涉及 `PrefixTracker` 签名变更。
- **变更 4**：Tool 化扩展（§10.4）—— SubagentTool / ExtensionTool / McpTool 等，按"先迁移后增强"原则。

---

**附录：盘点时的文件路径与关键行号**

- `crates/synthia-tool/src/traits.rs:13-72`
- `crates/synthia-tool/src/registry/registration/registry.rs:63-406`
- `crates/synthia-tool-bash/src/command_blacklist.rs:1-227`
- `crates/synthia-tool-bash/src/monitor.rs:7-65`
- `crates/synthia-hook/src/traits.rs:79-153`
- `crates/synthia-plugin/src/manifest.rs:48-130`
- `crates/synthia-plugin/src/hook_runner/mod.rs:1-60`
- `crates/synthia-skill/src/matcher/strategy.rs:24-45`
- `crates/synthia-skill/src/registry/lifecycle/activation.rs:25-96`
- `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs:107-983`
- `crates/synthia-agent/src/subagent/factory.rs:48-135`
- `crates/synthia-provider/src/cache_policy.rs:51-233`
- `crates/synthia-context/src/prefix_tracker/tracker.rs:1-242`
- `crates/synthia-context/src/compactor.rs:1-19`
- `crates/synthia-session/src/state_machine/machine.rs:18-132`
