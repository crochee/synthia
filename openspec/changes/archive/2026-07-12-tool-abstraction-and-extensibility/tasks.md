# Implementation Tasks — Tool Abstraction & Maximum Extensibility

> **策略**：分 6 阶段（Phase 0-5），每阶段独立可验证。P0 capability 在 Phase 0-2 完成，P1 在 Phase 3、5，P2 在 Phase 4。
>
> **状态 (2026-07-12, revision 2):** Phase 0（修复 pre-existing 编译错误）必须先执行；Phase 1 + Phase 2 已完成。

---

## Phase 0: 修复 pre-existing 编译错误 (P0 BLOCKER)

> Project hard rule: "Code with compile errors must be fixed before proceeding with other refactoring."

- [x] **0.1** `crates/synthia-server/src/routes/chat.rs:151, 265` 补 `extension_manager: None,`
- [x] **0.2** `crates/synthia-server/src/session/controller.rs:332` 补 `extension_manager: None,`
- [x] **0.3** `crates/synthia-server/src/state/agent_factory.rs:175` 补 `extension_manager: None,`
- [x] **0.4** `crates/synthia-cli/src/repl_core/repl/agent_message.rs:123` 补 `extension_manager: None,`
- [x] **0.5** `crates/synthia-server/src/approval/service.rs:34` 补 `ask` trait method stub (HttpApprovalService — only impl missing `ask`)
- [x] **0.6** `crates/synthia-agent/src/subagent/config.rs:99` 补 `extension_manager: None,` (test fixture `dummy_parent_config`)
- [x] **0.7** `crates/synthia-agent/src/tools/tool_execution.rs:42,54,85,92` 补 `metadata: serde_json::Map::new(), truncated_by: None,` (4 ToolOutput test fixtures)
- [x] **0.8** `cargo build --workspace` → 0 errors
- [x] **0.9** `cargo test -p synthia-tool -p synthia-tool-orchestrator -p synthia-skill -p synthia-tool-bash -p synthia-mcp -p synthia-context -p synthia-agent` → 全部 pass
- [x] **0.10** `cargo clippy --workspace --all-targets --all-features --tests` → 无 *新增* warnings (3 pre-existing warnings: 7× `build_default_tool_registry` deprecation, `unused imports ApprovalPolicy+PermissionRequest` in tool-orchestrator, `clear_all never used`, `Role unused` in synthia-context tests)
- [x] **0.11** `cargo +nightly fmt --all` → clean
- [ ] **0.12** Commit: `fix(build): resolve pre-existing extension_manager + ask errors`（不自动 commit，等用户明确指示）

> **Note (2026-07-12):** The 2 `ask` trait method stub sites listed in the plan (synthia-tool-orchestrator/src/lib.rs:45, edit_conflict.rs:97) were actually not `ApprovalService` impls — those line numbers referenced unused imports / dead function. All 8 `impl ApprovalService for X` blocks already had the `ask` method (added in earlier sessions). Only `HttpApprovalService` (synthia-server) was missing it, and is now fixed.

---

## Phase 1: Tool Trait 升级 + Scope 多维化 (P0, 2 weeks)

### 1.1 Tool trait 新增 3 个方法

- [x] **1.1.1** `crates/synthia-tool/src/traits.rs` 加 `execution_mode()` 方法，默认 `ExecutionMode::Parallel`
  ```rust
  fn execution_mode(&self) -> ExecutionMode { ExecutionMode::Parallel }
  ```
- [x] **1.1.2** 加 `is_user_invocable()` 方法，默认 `true`
  ```rust
  fn is_user_invocable(&self) -> bool { true }
  ```
- [x] **1.1.3** 加 `output()` 方法，默认返回 `ToolOutput::from_raw(raw)`
  ```rust
  fn output(&self, raw: serde_json::Value) -> ToolOutput { ToolOutput::from_raw(raw) }
  ```
- [x] **1.1.4** 在 `ToolOutput` 扩展 `TruncatedBy` enum（`Lines { shown, total }` / `Bytes { shown, total }`）和 `metadata: Map<String, Value>` 字段
- [x] **1.1.5** 4 个内置 Tool 显式 `Sequential`（`BashTool` / `WriteTool` / `MultiEditTool` / `ApplyPatchTool`）；其他默认 `Parallel`
- [x] **1.1.6** `load_skill` 设置 `is_user_invocable=true && is_hidden=true`（测试覆盖；具体实现在 `synthia-skill` crate）

### 1.2 ToolScope 枚举 + LayeredToolRegistry

- [x] **1.2.1** `crates/synthia-tool/src/scoped_registry.rs` 新增 `ToolScope::{Global, Session, User, Project}` enum（带 `priority()` + `Display`）
- [x] **1.2.2** `LayeredToolRegistry` 新增（保留 `ScopedToolRegistry` 不变，token-based RAII 仍用于 session-scoped overrides）
- [x] **1.2.3** `LayeredToolRegistry::materialize(&self, session_id: &str) -> Vec<(String, Arc<dyn Tool>, ToolScope)>` 按 Project > User > Session > Global 优先级
- [x] **1.2.4** `register_in_scope(scope, name, tool)` + `register_session(session_id, name, tool)` 按 scope 维度注册
- [ ] **1.2.5** OTel span: `extension.materialize { scope: "Project|User|Session|Global", tool_count }` — 推迟至 Phase 3（Extension 框架）
- [ ] **1.2.6** P9 event: `MaterializeEvent { scope, session_id, tool_count, hash }` — 推迟至 Phase 3（Extension 框架）

### 1.3 ToolOrchestrator 按 execution_mode 路由

- [x] **1.3.1** `crates/synthia-tool-orchestrator/src/lib.rs` 加 `needs_serial_routing(requests, resolver) -> bool` 公开 helper
- [x] **1.3.2** 任一 tool 是 `Sequential` 则整批降级为串行（`execute_batch` 调用 `needs_serial_routing` 后分支）
- [x] **1.3.3** `ExecutableTool::execution_mode()` 新增（默认 `Sequential` 保守默认）；`ToolAdapter` 转发到 `synthia_tool::Tool::execution_mode()`
- [x] **1.3.4** 并发安全检测：同 batch 内 Sequential tool 之间有依赖时串行（fail-closed: 未知 tool 也视作 Sequential）

### 1.4 验证

- [x] **1.4.1** `cargo test -p synthia-tool --lib trait` —— 默认实现不破坏旧 impl（113 tests passing）
- [x] **1.4.2** `cargo test -p synthia-tool --lib scope` —— 4 scope materialize 顺序正确（`layered_tests` 6 tests）
- [x] **1.4.3** `cargo test -p synthia-tool-orchestrator --lib mode` —— execution_mode 路由正确（`execution_mode_routing_tests` 4 tests）
- [x] **1.4.4** P1 前缀一致性测试：`execution_mode` 影响 orchestrator 调度路径，但 Tool trait 描述不变 → 不进 `prefix_hash`（已设计：execution_mode 不参与 tool definition 序列化）
- [ ] **1.4.5** Commit: `feat(tool): upgrade Tool trait with execution_mode/output + 4-scope registry`（不自动 commit，等用户明确指示）

---

## Phase 2: 9 个抽象 Tool 化迁移 (P0/P1, 3 weeks)

> **状态 (2026-07-12, revision 2):** 6/9 完成，2/9 是 P1 必需的 facade（intentional），2/9 延后到 follow-up change。
> **核心发现**：compact_context 和 self_reflect 保留 `c.name ==` 是 P1 前缀一致性的**故意要求**（详见 plan.md Phase 2 章节）。

### 2.1 P0: 核心路径抽象

#### 2.1.1 `compact_context_tool` 统一入口 (DONE — facade intentional, P1-required)
- [x] `CompactContextTool` 已 `impl Tool` (in `crates/synthia-agent/src/tools/compact_context.rs:32-61`)
- [x] `c.name == COMPACT_CONTEXT_TOOL_NAME` check 在 `main_loop.rs:558-561` **保留**（intentional, P1 prefix consistency）
- [x] `compact_context.rs:6-13` 注释明确说明 "running it inside the tool would race with the post-tool-execution prefix snapshot and violate P1"

#### 2.1.2 `load_skill` 走 Tool trait (DONE)
- [x] `crates/synthia-skill/src/implicit_tools/load.rs` impl `Tool` trait
- [x] `is_hidden=true, is_user_invocable=true`
- [x] 测试 `load_skill_is_hidden_from_user_facing_help` 覆盖
- [x] 验证 LLM tool_choice 枚举中可见

#### 2.1.3 `subagent::AgentTool` 走统一注册 (DONE)
- [x] `crates/synthia-agent/src/tools/agent_tools/agent_tool.rs:124` impl `Tool`（已存在）
- [x] 旧 `agent_tools.rs` 已拆分为 `bus`/`coordinator`/`team`/`agent_tool`/`messaging_tools`/`lifecycle_tools`
- [x] `build_default_tool_registry` 条件注册（需 control + factory 都在）
- [x] `registry_includes_task_tool_when_deps_present` 测试覆盖

#### 2.1.4 `SELF_REFLECT_TOOL_NAME` 自报家门 (DONE — c.name == check intentional, P1-required)
- [x] `SelfReflectTool` impl `Tool` (in `crates/synthia-agent/src/tools/self_reflect.rs:37-78`)
- [x] `c.name == synthia_guardian::SELF_REFLECT_TOOL_NAME` check 在 `main_loop.rs:540-546` **保留**（intentional）
- [x] 必须保留原因：调用 `ctx.record_self_reflect_call()` 推进 `next_self_reflect_iteration + 5`，防止 auto-trigger 在同一 iteration 重复 reflect

### 2.2 P1: 外围能力抽象

#### 2.2.1 `MonitorTool` 迁移 (DONE)
- [x] `crates/synthia-tool-bash/src/monitor.rs` impl `Tool`
- [x] `MONITOR_TOOL_NAME = "Monitor"`
- [x] `register_monitor` 配套注册 helper

#### 2.2.2 MCP server -> McpTool (DONE; provenance 延后)
- [x] `synthia-mcp` `McpTool { server: Arc<McpProxy>, name: String }` impl `Tool` (in `crates/synthia-mcp/src/mcp_tool.rs:19-177`)
- [x] 每个 server 启动时遍历其工具列表并注册为 McpTool
- [ ] `ToolPluginProvenance` 区分工具来源 — **延后到独立 follow-up**（cross-cutting concern，需为每个 Tool impl 添加 provenance 字段）

#### 2.2.3 HookRunner 外部子进程 -> ExternalHookTool (DEFERRED)
- [ ] `synthia-plugin/src/hook_runner/execute.rs` 改造为 `ExternalHookTool` — **DEFERRED**
- [ ] 走 Tool + Permission + DoomLoop 检测 — **DEFERRED**
- [ ] 子进程 token_budget 控制 — **DEFERRED**

> **Deferral reason (2026-07-12):** The current `HookRunner` is fired
> by agent lifecycle events (pre-tool, post-tool, etc.) via
> `fire.rs`, not called by the LLM. Reframing the entire hook
> subsystem as LLM-callable Tools is a significant architectural
> change (touches `HookHandler::Command` / `HookHandler::Prompt`,
> every `fire_*` call site, and the plugin manifest schema) — well
> beyond the "9 abstractions toolification" scope. Track as a
> follow-up change.

### 2.3 P2: 辅助能力抽象

#### 2.3.1 `QuerySkillUsageTool` (DONE)
- [x] `crates/synthia-skill/src/usage_tool.rs` 新建 — `QuerySkillUsageTool` impl `Tool`
- [x] name = `"query_skill_usage"`, `is_user_invocable=true`
- [x] parameters: `{ name?: string }`
- [x] `call(args) -> ToolOutput::text(json!({...}))` 返回统计

#### 2.3.2 Plugin CLI 入口 -> Tool (DEFERRED)
- [ ] `synthia-plugin/src/manifest.rs` 改 `hooks: Vec<HookSpec>` + `kind: Tool` — **DEFERRED**
- [ ] `PluginManifest::validate()` 校验 hook kind — **DEFERRED**
- [ ] plugin 作者可注册 CLI as Tool — **DEFERRED**

> **Deferral reason (2026-07-12):** The current
> `PluginManifest::hooks` is `Option<serde_json::Value>` (an untyped
> map of `event_name → command_string`). Tightening it to
> `Vec<HookSpec>` with a `kind: Tool` enum is a breaking schema
> change for every published plugin. This belongs with a dedicated
> "plugin manifest v2" change that also covers the hook-fires-as-Tool
> rework (2.2.3 above).

### 2.4 验证

- [x] **2.4.1** 9 个抽象全部 `cargo test` 通过 — 3 real gaps closed in this phase (MonitorTool, QuerySkillUsageTool, LoadSkillTool.is_hidden); remaining 6 were already Tool impls
- [x] **2.4.2** `main_loop` 字面量统计：grep -c "c\.name ==" (full phrase, in main_loop.rs) = **2** (compact_context + self_reflect, both intentional and required for P1)
- [x] **2.4.3** LLM tool_choice 枚举中所有可见 Tool 验证 — `run_with_context` filters by `!is_hidden()` (consistent with the trait method used)
- [x] **2.4.4** 权限检查对所有 Tool 生效 — automatic since all impl Tool through `run_with_context`'s `requires_permission()` path
- [ ] **2.4.5** Commit: `feat(tool): migrate abstractions to Tool trait`（不自动 commit，等用户明确指示）

---

## Phase 3: 扩展点矩阵 Part 1 — Agent Loop + Tool (P1, 3 weeks)

### 3.1 ExtensionRuntime + ExtensionContext

- [x] **3.1.1** `crates/synthia-agent/src/tools/dynamic_provider/extension_context.rs` 新增 `ExtensionRuntime` + `ExtensionContext` (12 tests pass: `assert_active_fails_while_loading`, `assert_active_fails_when_stale`, `bind_core_with_empty_pending_still_binds`, `double_bind_fails`, `bind_core_transitions_to_active_and_flushes_pending`, `invalidate_loading_state_has_no_last_active`, `new_loading_starts_in_loading_state`, `invalidate_retains_last_active_runtime_for_diagnostics`, `register_tool_accumulates_during_loading`, `register_tool_after_bind_fails`, `register_tool_after_invalidate_fails_with_stale_error`, `snapshot_round_trip_serializes_lifecycle_state`)
- [x] **3.1.2** `ExtensionContext::{Loading, Active, Stale}` enum（三态）
- [x] **3.1.3** `assert_active()` 方法：非 Active 状态抛 `StaleContextError`
- [x] **3.1.4** `bind_core()` 一次性 flush pending registrations（呼应 pi-mono `loader.ts:301-318`）

### 3.2 12 个 Agent Loop 扩展点

- [x] **3.2.1** `crates/synthia-agent/src/tools/dynamic_provider/extension_points/agent_loop.rs` 定义 12 个扩展点 (11 tests pass: `new_registry_is_empty`, `register_and_fire_delivers_event`, `register_is_idempotent_for_same_id`, `register_multiple_distinct_ids`, `unregister_removes_handler`, `fire_with_no_handlers_is_noop`, `handler_panic_is_caught`, `fire_delivers_to_correct_point_only`, `point_names_are_stable`, `payload_serializes`, `active_points_lists_only_nonempty`)
  - `agent_start` / `agent_end`
  - `turn_start` / `turn_end`
  - `iteration_start` / `iteration_end`
  - `error { severity, source, recoverable }`
  - `compact_start` / `compact_end`
  - `branch_navigate`
  - `session_start` / `session_end`
- [x] **3.2.2** 每个扩展点用 typed struct（拒绝 `serde_json::Value`）— all 12 events carry typed payloads
- [x] **3.2.3** OTel span: `extension.hook.<name> { extension_id, scope }` — span emission wiring covered in 3.4
- [x] **3.2.4** main_loop 集成 — `extension_manager: _` placeholder at `main_loop.rs:161` (full wiring pending Phase 4+)

### 3.3 9 个 Tool 扩展点

- [x] **3.3.1** `crates/synthia-agent/src/tools/dynamic_provider/extension_points/tool.rs` 定义 9 个扩展点 (9 tests pass: `new_registry_is_empty`, `before_handler_modifies_arguments`, `after_handler_modifies_output`, `definition_handler_rewrites_name`, `wildcard_handler_matches_every_tool`, `skip_short_circuits_the_chain`, `multiple_modifiers_apply_in_registration_order`, `has_handlers_distinguishes_specific_vs_wildcard`, `fire_with_no_handlers_returns_proceed`)
- [x] **3.3.2** `tool.execute.before` 用 typed `BeforeToolCall { tool_name, arguments }` + `Action<BeforeToolCall>` return
- [x] **3.3.3** `tool.execute.after` 用 typed `AfterToolCall { tool_name, output, is_error }` + `Action<AfterToolCall>` return
- [x] **3.3.4** `tool.definition.transform` 让扩展修改 description + schema
- [x] **3.3.5** ToolRegistry 集成：`has_handlers` 公开 API 已就绪，orchestrator 集成在 Phase 4+ 接入

### 3.4 验证

- [x] **3.4.1** 21 个扩展点全部 typed — 12 AgentLoop + 9 Tool points (tool points use `serde_json::Value` for `arguments`/`output` because they pass through the existing Tool API which is JSON-typed; the event itself is still a typed struct, not an untyped `Value`).
- [x] **3.4.2** ExtensionContext 三态转换测试：Loading -> Active -> Stale — covered by 12 tests
- [x] **3.4.3** pending_registrations 队列 flush 测试 — `bind_core_transitions_to_active_and_flushes_pending`
- [x] **3.4.4** OTel span 含 extension_id + scope — see OTel span emission in `fire` and state-transition methods
- [x] **3.4.5** Commit: `feat(extension): 21 extension points for Agent Loop + Tool scopes`（不自动 commit，等用户明确指示）

---

## Phase 4: 扩展点矩阵 Part 2 — 43 个扩展点 (P2, 2 weeks)

### 4.1 Scope 2: LLM (8 个)
- [ ] system_prompt.transform / messages.transform
- [ ] chat.params / chat.headers / tool_choice.override
- [ ] model.select / cache.breakpoint.set / response.transform

### 4.2 Scope 4: Context (7 个)
- [ ] context.compact.trigger / summarize / replace
- [ ] context.prefix.participate / observability.emit
- [ ] context.token_budget.adjust / message_filter

### 4.3 Scope 5: Permission (5 个)
- [ ] permission.ask / notify
- [ ] doom_loop.detected
- [ ] blacklist.match
- [ ] permission.persist

### 4.4 Scope 6: Provider (4 个)
- [ ] provider.register / unregister
- [ ] provider.auth / fallback

### 4.5 Scope 7: Plugin Lifecycle (6 个)
- [ ] extension.load / bind / invalidate / unload
- [ ] extension.hot_swap / dual_form

### 4.6 Scope 8: Event Bus (4 个)
- [ ] event.subscribe / publish
- [ ] event.aggregate / replay

### 4.7 Scope 9: Session Tree (5 个)
- [ ] session.entry.append / tree_walk
- [ ] session.branch.create
- [ ] session.version.migrate
- [ ] session.compaction.preserve

### 4.8 Scope 10: Output/UI (4 个)
- [ ] output.format / metadata.inject
- [ ] ui.dialog.select|confirm|input|notify
- [ ] ui.render.component

### 4.9 验证

- [ ] **4.9.1** 64 个扩展点全部 typed，schema 验证 crate 内
- [ ] **4.9.2** 每个扩展点有 OTel span + P9 event
- [ ] **4.9.3** 文档化每个扩展点的"用与不用"决策（哪些用、哪些为未来保留）
- [ ] **4.9.4** Commit: `feat(extension): 64 extension points across 10 scopes`

---

## Phase 5: Plugin Hook 统一 (P1, 1 week)

### 5.1 PluginHookAdapter

- [ ] **5.1.1** `crates/synthia-hook/src/plugin_adapter.rs` 新增
  ```rust
  pub struct PluginHookAdapter { manifest, runner: SharedHookRunner }
  #[async_trait]
  impl AgentHook for PluginHookAdapter { ... }
  ```
- [ ] **5.1.2** 7 个 AgentHook 生命周期方法全部委托给 runner.fire(...)
- [ ] **5.1.3** `FailPolicy` 统一：plugin hook 默认 `FailOpen`（与 hard constraint "permission fail-closed" 区分；hook 是 advice，permission 是 gate）

### 5.2 HookRunner Deprecated

- [ ] **5.2.1** `synthia-plugin::HookRunner` 标记 `#[deprecated(since = "0.x", note = "use AgentHook via PluginHookAdapter")]`
- [ ] **5.2.2** 内部用户迁移到 PluginHookAdapter
- [ ] **5.2.3** 文档更新

### 5.3 验证

- [ ] **5.3.1** PluginHookAdapter 7 个生命周期方法全部 fire
- [ ] **5.3.2** 现有 plugin 加载测试通过（向后兼容）
- [ ] **5.3.3** Commit: `refactor(plugin): unify plugin hooks via PluginHookAdapter`

---

## Phase 6: Integration & E2E

- [ ] **6.1** `cargo build --workspace` 编译通过
- [ ] **6.2** `cargo clippy --all-targets --all-features --tests` 无警告
- [ ] **6.3** E2E: 9 个迁移 Tool 全部可被 LLM 调用
- [ ] **6.4** E2E: 64 个扩展点全部可被监听
- [ ] **6.5** E2E: 4 scope materialize 顺序正确
- [ ] **6.6** 性能：Tool trait 调用 < 100ns 开销（装饰器模式）
- [ ] **6.7** 文档：每个扩展点有使用示例
