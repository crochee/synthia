## Context

Synthia 当前 Tool 系统是"窄门"——只有 7 个内置 Tool + 1 个 Scoped 包装。大量"非核心能力"以分散方式散落在各 crate，导致：
- LLM 不能直接发现/调用
- 每次新增能力都要改 `main_loop` 特殊分支
- 第三方插件作者无法注册
- 权限/循环检测/Hook 路径对它们不生效

**生产级对照**：
- opencode 19 hook + dual plugin kind（server / tui）—— [`packages/plugin/src/index.ts:74-80`](file:///home/crochee/workspace/opencode/packages/plugin/src/index.ts#L74-L80)
- codex 10 event + ToolRouter + McpConnectionManager + CommandEnvironment —— [`codex-rs`](file:///home/crochee/workspace/codex)
- pi-mono 20+ extension point + ToolDefinition↔AgentTool 双形态 —— [`pi-mono/BORROWABLE_PATTERNS.md`](file:///home/crochee/workspace/pi-mono/BORROWABLE_PATTERNS.md)

**目标**：把"扩展性"和"Tool 抽象纯粹度"作为一等公民。任何非核心能力都能在不改 main_loop、不改 ToolRegistry 的前提下注册为 Tool。

## Goals / Non-Goals

**Goals：**
- Tool trait 成为"通用能力接口"，所有非核心能力都通过它暴露
- 4 scope × 30+ 扩展点统一注册
- 强边界：react loop + session 之外的所有能力都抽象为 Tool
- 第三方插件作者可注册上述所有扩展点
- 保持 P1-P10 原则不退化

**Non-Goals：**
- 不实现 Effect-rs 全栈（仅借鉴 Scope 概念）
- 不重构 core ReAct loop 本身
- 不引入 V8/WASM 沙箱（独立子 change）
- 不做 UI 端扩展（仅 server 端扩展点）

## Architecture Overview

```
┌────────────────────────────────────────────────────────────┐
│  ReAct Loop + Session（核心，不在本 change 内重构）         │
└────────────────────────────────────────────────────────────┘
                            │ Tool call
                            ▼
┌────────────────────────────────────────────────────────────┐
│  ToolRegistry：4-scope Materialize                          │
│  ┌──────────┐ ┌──────────┐ ┌──────┐ ┌──────────┐          │
│  │ Project  │▶│   User   │▶│Session│▶│  Global  │          │
│  └──────────┘ └──────────┘ └──────┘ └──────────┘          │
│       priority:  P_high    P_normal  P_low                 │
└────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────────┐
│  Tool trait（统一接口）                                      │
│  - execution_mode (Parallel/Sequential)                     │
│  - is_user_invocable / is_hidden                           │
│  - output: ToolOutput { content, metadata, truncated_by }  │
│  - call_with_sandbox(input, sandbox, &ToolContext)         │
│  - ToolContext { cancel_token, extension_ctx,              │
│                  directory, worktree, abort, ask, metadata}│
└────────────────────────────────────────────────────────────┘
            ▲                ▲                ▲
            │                │                │
┌───────────┴──┐  ┌──────────┴──┐  ┌──────────┴──┐
│ 7 个内置 Tool │  │ 9 个迁移 Tool │  │ N 个扩展 Tool│
│ read/write/  │  │ load_skill  │  │ 由 plugin   │
│ glob/grep/   │  │ subagent    │  │ 作者注册    │
│ multi_edit/  │  │ self_reflect│  │             │
│ apply_patch/ │  │ compact     │  │             │
│ web_fetch    │  │ mcp_*       │  │             │
│              │  │ monitor     │  │             │
│              │  │ query_skill │  │             │
│              │  │ ext_hook_*  │  │             │
│              │  │ plugin_*    │  │             │
└──────────────┘  └─────────────┘  └─────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────────┐
│  Extension Runtime（ExtensionTool 形态）                   │
│  - ExtensionRuntime { register_tool, send_message,         │
│                        append_entry, ui_dialog, ...}       │
│  - ExtensionContext { session_id, agent, runtime_ref }     │
│  - Loading / Active / Stale 三态                          │
│  - Pending registration queue + bind_core flush            │
└────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────────┐
│  Event Bus + Extension Point Matrix                        │
│  - 4 scope × 30+ 扩展点（见 §3 矩阵）                      │
│  - 强类型 event（typed publish / subscribe）               │
│  - 序列号、aggregate id、metadata 显式建模                 │
└────────────────────────────────────────────────────────────┘
```

## Decisions

### D1: Tool trait 升级 —— 3 个新方法 + 1 个新结构

**选择**：在 `synthia-tool::Tool` 加 3 个方法（带默认实现）+ 1 个返回结构 `ToolOutput`：

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    // 现有 7 个方法保持不变
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    fn requires_permission(&self) -> bool { false }
    fn is_hidden(&self) -> bool { false }
    fn is_concurrency_safe(&self) -> bool { false }
    async fn call(&self, input: ToolInput) -> ToolOutput;
    async fn call_with_sandbox(&self, ...) -> ToolOutput { ... }
    async fn call_with_progress(&self, ...) -> ToolOutput { ... }

    // 新增（带默认实现，外部 impl 可选 override）
    fn execution_mode(&self) -> ExecutionMode { ExecutionMode::Parallel }
    fn is_user_invocable(&self) -> bool { true }  // LLM 可见
    fn output(&self, raw: serde_json::Value) -> ToolOutput {
        // 默认：content = raw, metadata = {}, truncated_by = None
    }
}

pub enum ExecutionMode { Parallel, Sequential }

pub struct ToolOutput {
    pub content: String,
    pub metadata: serde_json::Map<String, serde_json::Value>,
    pub truncated_by: Option<TruncatedBy>,
}
pub enum TruncatedBy { Lines { shown, total }, Bytes { shown, total } }
```

**理由**：
- **execution_mode**：参考 pi-mono [`packages/agent/src/agent-loop.ts:338-353`](file:///home/crochee/workspace/pi-mono/packages/agent/src/agent-loop.ts#L338-L353)，让 tool 自报"串行/并行"
- **is_user_invocable**：与 is_hidden 解耦。is_hidden=true 但 is_user_invocable=true 的 tool（如 load_skill）LLM 可见但不出现在 help 中
- **output()**：强制结构化输出，包含截断元信息（呼应 pi-mono `TruncationResult` 的设计）

**已考虑 alternative**：
- A. 用 `dyn Tool` + type erasure 完全不升级 trait —— 拒绝，因为无法在 trait 上做 compile-time 优化
- B. 新建 `ToolV2` trait，与 `Tool` 并存 —— 拒绝，引入双 trait 增加心智负担

### D2: 4 Scope × 30+ 扩展点矩阵

**选择**：从 opencode 19 hook + codex 10 event + pi-mono 20+ extension point + synthia 7 AgentHook 中去重、归类、严格按 4 scope 分层：

**Scope 1: Agent Loop（12 个）**
- `agent_start` / `agent_end`
- `turn_start` / `turn_end`
- `iteration_start` / `iteration_end`
- `error { severity, source, recoverable }`
- `compact_start { reason: Manual|Threshold|Overflow }` / `compact_end`
- `branch_navigate { from_id, to_id }`
- `session_start` / `session_end`

**Scope 2: LLM（8 个）**
- `system_prompt.transform`
- `messages.transform`
- `chat.params { temperature, top_p, top_k, max_tokens }`
- `chat.headers.inject`
- `tool_choice.override`
- `model.select`
- `cache.breakpoint.set`
- `response.transform`

**Scope 3: Tool（9 个）**
- `tool.execute.before(args) -> Action<args>`
- `tool.execute.after(output) -> Action<output>`
- `tool.definition.transform(description, schema)`
- `tool.registry.register`
- `tool.registry.unregister`
- `tool.execution_mode.override`
- `tool.parallelism.barrier`
- `tool.output.format`
- `tool.output.metadata.inject`

**Scope 4: Context / Compaction（7 个）**
- `context.compact.trigger(reason)`
- `context.compact.summarize(head, previous) -> summary`
- `context.compact.replace(entries) -> replacement`
- `context.prefix.participate(entries) -> hash_bytes`
- `context.observability.emit(event)`
- `context.token_budget.adjust(usage)`
- `context.message_filter(entries)`

**Scope 5: Permission（5 个）**
- `permission.ask(request) -> Decision`
- `permission.notify { status, reason }`
- `doom_loop.detected(signature)`
- `blacklist.match(command) -> Warning`
- `permission.persist { mode: Always|Once|Deny }`

**Scope 6: Provider（4 个）**
- `provider.register(lazy)`
- `provider.unregister`
- `provider.auth(oauth|apikey)`
- `provider.fallback`

**Scope 7: Plugin Lifecycle（6 个）**
- `extension.load(pending)`
- `extension.bind(flush)`
- `extension.invalidate(mark_stale)`
- `extension.unload(cleanup)`
- `extension.hot_swap(reload)`
- `extension.dual_form(agent|extension)`

**Scope 8: Event Bus（4 个）**
- `event.subscribe(topic)`
- `event.publish(event)`
- `event.aggregate(id, version)`
- `event.replay(from_sequence)`

**Scope 9: Session Tree（5 个）**
- `session.entry.append(entry)`
- `session.entry.tree_walk(leaf)`
- `session.branch.create(parent_id)`
- `session.version.migrate(from, to)`
- `session.compaction.preserve(from_hook)`

**Scope 10: Output/UI（4 个）**
- `output.format(text) -> LLM_visible`
- `output.metadata.inject(data)`
- `ui.dialog.select|confirm|input|notify`
- `ui.render.component`

**总计：12 + 8 + 9 + 7 + 5 + 4 + 6 + 4 + 5 + 4 = 64 个扩展点**

**理由**：
- **64 = 强矩阵** —— 任何新能力都能找到归属
- **按 scope 分层** —— 与 P10 文件即记忆一致：scope 决定 extension 的"作用域"
- **typed contract** —— 拒绝"string-keyed map"形式（如 `hooks: serde_json::Value`），用 enum + struct 强制 schema

**已考虑 alternative**：
- A. 保持 19 个 hook 不增不减 —— 拒绝，用户诉求是"极大扩展性"
- B. 100+ 扩展点 —— 拒绝，过度设计；64 是"既覆盖所有能力又不过度"的甜点

### D3: 双形态 Extension（ToolDefinition ↔ AgentTool）

**选择**：参考 pi-mono [`packages/coding-agent/src/core/tools/tool-definition-wrapper.ts:5-44`](file:///home/crochee/workspace/pi-mono/packages/coding-agent/src/core/tools/tool-definition-wrapper.ts#L5-L44)：

```rust
// 核心形态（AgentTool）—— 无扩展上下文
#[async_trait]
pub trait Tool: Send + Sync { /* D1 接口 */ }

// 扩展形态（ExtensionTool）—— 含 ExtensionContext
#[async_trait]
pub trait ExtensionTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    async fn execute(&self, input: serde_json::Value, ctx: ExtensionContext) -> Result<ToolOutput>;
    fn ui_render(&self) -> Option<...>;  // 选填
}

// 装饰器：ExtensionTool -> Tool
pub struct ToolAdapter {
    inner: Arc<dyn ExtensionTool>,
    ctx_factory: Arc<dyn Fn() -> ExtensionContext + Send + Sync>,
}
impl ToolAdapter {
    pub fn new(inner: Arc<dyn ExtensionTool>, ctx_factory: ...) -> Arc<dyn Tool> { ... }
}
impl Tool for ToolAdapter {
    async fn call(&self, input: ToolInput) -> ToolOutput {
        let ctx = (self.ctx_factory)();
        self.inner.execute(input.args, ctx).await?
    }
}

// 反向：Tool -> ExtensionTool（最小合成）
pub fn create_extension_tool_from_agent(tool: Arc<dyn Tool>) -> Arc<dyn ExtensionTool> { ... }
```

**理由**：
- **关注点分离**：核心 tool 不需要"知道扩展存在"
- **`ctx_factory` 而非 `ctx`**：延迟构造 ExtensionContext，呼应 pi-mono `wrapToolDefinition` 的设计
- **装饰器模式**：`ToolAdapter` 是无损转换，运行时多一层 call 但无额外分配

### D4: 4 Scope 隔离（per-session + per-user + per-project + global）

**选择**：升级 `ScopedToolRegistry`（来自 production-grade change）为 4 维 scope：

```rust
pub enum ToolScope {
    Global,    // 进程级默认
    Session,   // 单个 session 内
    User,      // 用户级配置（~/.config/synthia/tools.toml）
    Project,   // 项目级（.synthia/tools.toml）
}

pub struct LayeredToolRegistry {
    layers: Vec<(ToolScope, ToolRegistry)>,  // 按优先级排序
}

impl LayeredToolRegistry {
    pub fn materialize(&self, session_id: &str) -> Vec<ToolEntry> {
        // Project > User > Session > Global
        // 同名 tool 取优先级最高 + last-wins within layer
    }

    pub fn register_in_scope(&self, scope: ToolScope, name: String, tool: Arc<dyn Tool>) { ... }
}
```

**理由**：
- **4 维 vs 1 维**：对应 Linux 配置文件查找顺序（system-wide / user / project-local）
- **materialize() 一次性计算** + 缓存（cache key = `session_id`）
- **P9 可观测**：每次 materialize 发 `extension.materialize` 事件

**已考虑 alternative**：
- A. 只做 Session scope（production-grade 已实现）—— 拒绝，不够用
- B. 任意自由链式 scope（无层级）—— 拒绝，调试时无法预测优先级

### D5: ExtensionContext —— 加载期 vs 运行期

**选择**：参考 pi-mono [`packages/coding-agent/src/core/extensions/loader.ts:134-180`](file:///home/crochee/workspace/pi-mono/packages/coding-agent/src/core/extensions/loader.ts#L134-L180)，三态 enum：

```rust
pub enum ExtensionContext {
    Loading {
        session_id: SessionId,
        // 加载期只能 register_*，不能 send_message
        register_tool: Box<dyn Fn(Arc<dyn Tool>) + Send>,
        register_provider: Box<dyn Fn(...) + Send>,
        register_flag: Box<dyn Fn(...) + Send>,
    },
    Active {
        session_id: SessionId,
        runtime: Arc<ExtensionRuntime>,
        // 运行期可以 send_message / append_entry / ui_dialog
    },
    Stale {
        reason: String,  // session 替换后所有 ctx 变 Stale
    },
}

impl ExtensionContext {
    pub fn assert_active(&self) -> Result<&Active> {
        match self { Active(a) => Ok(a), _ => Err(...) }
    }
}
```

**理由**：
- **三态 enum** 强制状态转换：Loading → Active（bind_core 时）→ Stale（session 替换时）
- **fail-fast**：未初始化 action 调用立刻抛错（`assert_active()`）
- **pending registration queue** 配合 bind_core 一次性 flush

### D6: 9 个抽象 Tool 化的迁移路径

**选择**：逐个迁移到 `Tool` trait + 走 `ToolRegistry`：

| # | 抽象 | 迁移步骤 | 验收标准 |
|---|------|----------|----------|
| 1 | `compact_context_tool` | 已是 Tool，统一调用入口（替换 `main_loop.rs:555-561` 的 facade） | `main_loop` 不再有 `compact_context_tool` 字面量 |
| 2 | `load_skill` | 走 Tool trait + `is_hidden=true` + `is_user_invocable=true` | LLM 可见但不出现在 help |
| 3 | `subagent::AgentTool` | 复用 ToolRegistry，删除 `agent_tools.rs` 双轨 | subagent 走 `registry.run_with_context` |
| 4 | `SELF_REFLECT_TOOL_NAME` | 自报家门（const NAME），main_loop 用 `tool.name() == ...` | 替换 `main_loop.rs:543-546` 字面量 |
| 5 | `MonitorTool` | 迁移到 Tool trait，注册到 ToolRegistry | bash 工具集合中可见 |
| 6 | 每个 `McpProxy` server | `McpTool { server: Arc<McpProxy>, name: String }` | server 启动后自动注册 |
| 7 | `HookRunner` 外部子进程 | `ExternalHookTool { command, args, token_budget }` | 走 Tool + Permission + DoomLoop |
| 8 | `synthia-skill::usage` | `QuerySkillUsageTool::call` 返回 JSON | LLM 可查询 skill 统计 |
| 9 | Plugin CLI 入口 | `manifest.hooks: Vec<HookSpec>` + `kind: Tool` | plugin 作者可注册 CLI as Tool |

**理由**：
- **9 个全部 P0/P1**：避免"半迁移"导致技术债
- **验收标准量化**：每步有可测量的代码变化
- **顺序依赖**：1→2→3→4 是 P0（核心路径），5-9 是 P1/P2（外围能力）

### D7: Plugin Hook 统一 —— AgentHook + HookRunner 合并

**选择**：把 `synthia-plugin::HookRunner` 全部转化为 `synthia-hook::AgentHook` impl：

```rust
// 新增：synthia-hook/src/plugin_adapter.rs
pub struct PluginHookAdapter {
    manifest: PluginManifest,
    runner: SharedHookRunner,
}

#[async_trait]
impl AgentHook for PluginHookAdapter {
    async fn on_before_llm(&self, ctx: &mut AgentContext) -> Result<(), HookError> {
        // 委托给 runner.fire("chat.message", ...)
    }
    async fn on_before_tool(&self, tool: &str, args: &mut Value) -> Result<ToolAction, HookError> {
        // runner.fire("tool.execute.before", ...)
    }
    // ... 7 个生命周期方法
}
```

**理由**：
- **统一接口**：AgentHook 已经是 7 个生命周期方法，足够覆盖 plugin 19 hook 的常见用法
- **避免双系统**：开发者只需学一套
- **保留 plugin 命令式子进程能力**：`HookRunner` 仍存在，作为 AgentHook 的实现细节

**已考虑 alternative**：
- A. 完全删除 `synthia-plugin::HookRunner` —— 拒绝，命令式子进程在 plugin 场景仍有价值
- B. 保留双系统并显式标注使用场景 —— 拒绝，加重开发者选择负担

## Risks / Trade-offs

**[Risk] Tool trait 升级破坏外部 impl** → 缓解：3 个新增方法都带默认实现；文档化为 "soft break"，旧 impl 编译通过但行为默认。

**[Risk] 64 个扩展点过度设计** → 缓解：分 4 期实施（每期 16 个），每期评估"用到的比例"；< 30% 使用率则停掉后续期。

**[Risk] ExtensionContext 三态增加状态机复杂度** → 缓解：enum 强制状态转换，编译期拒绝错误用法。

**[Risk] 4 scope 隔离的优先级调试困难** → 缓解：materialize 时发 P9 事件，OTel span 含 `scope: Project|User|Session|Global` 标签。

**[Trade-off] 双形态装饰器 vs 单一 trait** → 装饰器多一层 call（~50ns），换来关注点分离和扩展性；接受 trade-off。

**[Trade-off] 9 个抽象一次性迁移 vs 渐进** → 一次性可避免"半迁移状态"；但需 4-6 周完成。

## Migration Plan

### Phase 1: Tool trait 升级 + Scope 升级（P0，2 周）
1. `Tool` trait 加 3 个新方法（默认实现）
2. `LayeredToolRegistry` 替代 `ScopedToolRegistry`
3. 所有内置 Tool 加 `execution_mode` 声明
4. P1 前缀一致性测试（execution_mode 不影响 hash）

### Phase 2: 9 个抽象 Tool 化（P0/P1，3 周）
1. compact_context_tool + load_skill + subagent + self_reflect（P0）
2. MonitorTool + McpTool + ExternalHookTool（P1）
3. QuerySkillUsageTool + PluginCLI as Tool（P2）

### Phase 3: 扩展点矩阵实施（P1，3 周）
1. ExtensionRuntime + ExtensionContext（Loading/Active/Stale）
2. 64 个扩展点中前 16 个（Agent Loop + Tool）
3. OTel span 标签、event publish

### Phase 4: 扩展点矩阵扩展（P2，2 周）
1. 后 48 个扩展点（LLM/Context/Permission/Provider/Plugin Lifecycle/Event Bus/Session Tree/Output）
2. 文档化每个扩展点的"用与不用"

### Phase 5: Plugin Hook 统一（P1，1 周）
1. `PluginHookAdapter` 实现
2. `synthia-plugin::HookRunner` 标记 deprecated

**Rollback Strategy**：
- P0：Tool trait 软升级，新方法默认实现，外部 impl 不更新也能编译（仅行为默认）
- P1：扩展点矩阵分 4 期，每期独立可回滚
- P2：9 个抽象按 P0/P1/P2 分批，先 P0 后 P1/P2

## Open Questions

1. **是否强制 P1 前缀一致性包含 `execution_mode`？** —— execution_mode 影响 orchestrator 调度路径，但不进 LLM context。建议：不进 hash（execution_mode 是 orchestrator 内部状态）

2. **64 个扩展点的 schema 验证放 crate 内还是插件作者自验？** —— 建议：crate 内 `ExtensionSpec` derive `schemars`，启动期校验

3. **Plugin 的 Tool 注册是 process 级还是 per-session？** —— 建议：双层，process 级（plugin 启动时）+ per-session override（用户在 session 内禁用某 tool）

4. **`load_skill` 应该走 `is_hidden=true` 还是 `is_user_invocable=false`？** —— 两者都满足；建议 `is_user_invocable=true && is_hidden=true`（LLM 可见但不出现在 help 中）

5. **`query_skill_usage` 是否计入 P1 hash？** —— 建议：计入（query 结果可能影响后续 prompt 注入）
