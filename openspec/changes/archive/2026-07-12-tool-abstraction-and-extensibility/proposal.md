## Why

Synthia 当前 `synthia-tool::Tool` trait 是 **核心 agent 能力的统一入口**，但大量非核心能力（load_skill / subagent / guard self_reflect / MCP server / plugin CLI / skill usage / 外部子进程 hook 等）仍以 **private crate 抽象** 形式存在，没有走 `ToolRegistry`，导致：
- LLM 不能直接发现/调用这些能力
- 每次新增能力都要改 `main_loop` 的特殊分支（`c.name ==` 字面量比较散落各处）
- 第三方插件作者无法注册这些能力
- 权限/循环检测/Hook 路径对它们不生效

对比生产级 agent（opencode 19 hook、codex 10 event、pi-mono 20+ extension point），synthia 的扩展性矩阵在 **覆盖广度** 和 **Tool 抽象的纯粹度** 两方面均有显著 gap。

**目标**：把"扩展性"和"Tool 抽象纯粹度"作为一等公民，让后续任何能力（agent loop 之外）都能在不改 main_loop、不改 ToolRegistry 的前提下注册为 Tool。

## What Changes

**A. 统一 Tool 抽象层（Tool trait 升级）**
- `Tool::execution_mode(): ExecutionMode`（默认 `Parallel`，`BashTool`/`WriteTool`/`EditTool` 声明 `Sequential`）
- `Tool::is_user_invocable(): bool`（默认 `true`；`load_skill` 设为 `true` 但 `is_hidden()=true`）
- `Tool::cancel_token_async_signal(): Option<...>`（标准 yield point 行为）
- `Tool::output(): ToolOutput { content, metadata }` —— 强制结构化输出（line 200 / byte 50K / 截断原因）

**B. 扩展点矩阵（4 scope × 30+ 扩展点）**

合并 opencode（19 hook）+ codex（10 event）+ pi-mono（20+ extension point）+ synthia 现状（7 AgentHook），去重、归类、严格按 4 scope 分层：

| Scope | 扩展点数量 | 主要类别 |
|------|-----------|----------|
| Agent Loop | 12 | turn/iter/compact/session/error/branch |
| LLM | 8 | system/messages/params/headers/tools/cache/model |
| Tool | 9 | execute.before/after/definition/registry/execution |
| Context | 7 | compact.trigger/summarize/replace/prefix/observability |
| Permission | 5 | ask/notify/doom_loop/blacklist/persist |
| Provider | 4 | register/unregister/auth/lazy |
| Plugin Lifecycle | 6 | load/bind/invalidate/unload/hot_swap/dual-form |
| Event Bus | 4 | subscribe/publish/aggregate/sequence |
| Session Tree | 5 | entry/append/tree_walk/branch/version |
| Output/UI | 4 | format/metadata/dialog/render |

**C. 9 个现有抽象 Tool 化迁移**

| # | 现有抽象 | 迁移目标 | 优先级 |
|---|---------|---------|--------|
| 1 | `synthia-context::compact_context_tool` | 已经是 Tool，统一调用入口 | P0 |
| 2 | `synthia-skill::implicit_tools::load_skill` | 走 Tool trait + `is_hidden=true` | P0 |
| 3 | `synthia-agent::subagent::AgentTool` | 复用 `ToolRegistry`，去掉双轨 | P0 |
| 4 | `synthia-guardian::SELF_REFLECT_TOOL_NAME` | 自报家门，去掉字面量比较 | P0 |
| 5 | `synthia-tool-bash::MonitorTool` | 迁移到 `Tool` trait | P1 |
| 6 | 每个 `McpProxy` server | 实例化为 `McpTool { server, name }` | P1 |
| 7 | `HookRunner` 外部子进程 | 统一为 Tool（副作用：token-budgeted） | P1 |
| 8 | `synthia-skill::usage` tracker | 暴露为 `query_skill_usage` Tool | P2 |
| 9 | Plugin CLI 入口 | `manifest.hooks: Vec<HookSpec>` + `kind: Tool` | P2 |

**D. 双形态 Extension（ToolDefinition ↔ AgentTool）**

参考 pi-mono `wrapToolDefinition` / `createToolDefinitionFromAgentTool`：
- `Tool`（核心，sync，无扩展上下文）
- `ExtensionTool`（含 `ExtensionContext`）
- `ToolAdapter::from_extension(ext) -> Arc<dyn Tool>` —— 装饰器
- `ToolDefinition::from_agent(tool) -> ToolDefinition` —— 反向

**E. Scope 隔离（per-session 工具命名空间）**

复用现有 `ScopedToolRegistry`，升级为"4 个 scope 维度"：
- `Global`：进程级默认
- `Session`：单个 session 内
- `User`：用户级配置
- `Project`：项目级（`.synthia/tools.toml`）

`materialize()` 优先级：`Project > User > Session > Global`，并发出 P9 事件。

## Capabilities

### New Capabilities

- `tool-trait-universal`: Tool trait 升级为"通用能力接口"：execution_mode / user_invocable / structured output / 标准 yield point
- `extension-point-matrix`: 4 scope × 30+ 扩展点的统一注册中心，所有非核心能力都通过这个矩阵暴露
- `scope-isolation`: 4 个 scope 维度的 ToolRegistry 隔离（Global/Session/User/Project）
- `extension-dual-form`: Tool（核心）↔ ExtensionTool（含 ExtensionContext）双形态 + 装饰器互转
- `plugin-unification`: 把 `AgentHook`（`synthia-hook`）和 `HookRunner`（`synthia-plugin`）两套 hook 系统合并为单接口
- `9-abstractions-toolification`: 9 个现有抽象（compact/load_skill/subagent/guard/monitor/MCP/HookRunner/usage/PluginCLI）迁移为标准 Tool

### Modified Capabilities

- `tool-cancellation-propagation` (existing in production-grade change): 扩展为接受标准 `ToolContext`（含 `cancel_token` + `extension_ctx` + `directory` + `worktree`）
- `scoped-tool-registry` (existing in production-grade change): 升级为 4 维 scope（不仅 session）
- `doom-loop-proactive-detection` (existing in production-grade change): 加入跨 scope 计数（per-session / per-tool / global）

## Impact

### Affected Crates

| Crate | Changes |
|-------|---------|
| `synthia-tool` | Tool trait 升级（execution_mode, user_invocable, output struct）；新增 `extension.rs` / `dual_form.rs` / `scope.rs` |
| `synthia-tool-orchestrator` | 按 execution_mode 路由并行/串行；4-scope materialize |
| `synthia-extension` (新) | 4 scope × 30+ 扩展点的注册中心；ExtensionContext；ExtensionRuntime |
| `synthia-hook` | 与 `synthia-plugin::HookRunner` 合并为单接口 |
| `synthia-plugin` | 移除独立 HookRunner；manifest.hooks 加 `kind: Tool` |
| `synthia-skill` | `load_skill` 走 Tool trait；`usage` 暴露为 Tool |
| `synthia-agent` | `subagent` 走 `ToolRegistry`；main_loop 移除所有 `c.name ==` 字面量 |
| `synthia-guardian` | `self_reflect` 走 Tool trait |
| `synthia-mcp` | 每个 server 实例化为 `McpTool` |
| `synthia-context` | `compact_context_tool` 走统一接口 |

### Breaking Changes

- **`Tool` trait 新增 3 个方法**（`execution_mode` / `is_user_invocable` / `output`）—— 所有 Tool 实现需更新（提供默认实现，外部 impl 显式 override）
- **删除 `synthia-plugin::HookRunner` 公共 API**（合并到 `synthia-hook`）—— 内部用户需更新
- **`subagent` 不再走 `agent_tools.rs` 双轨**—— 走统一 `ToolRegistry`

### Non-Breaking

- 现有 7 个内置 Tool 通过 trait 默认实现兼容（无需修改）
- `ScopedToolRegistry` API 升级为 4-scope，旧用法自动映射到 `Session` scope
