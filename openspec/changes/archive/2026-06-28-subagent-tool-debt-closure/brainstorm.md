<!--
Raw capture of brainstorming output.

本檔原樣捕捉多专家对抗性分析的决策链，不強制結構。
Skill 的自然產出通常是 decision log 格式（背景 → 決議鏈 Q1-Qn → 設計取捨），
但依對話內容可能有不同組織方式。

design.md 從本檔萃取並重新整理為結構化設計文件。

不要將本檔的內容複製到 design.md — design.md 是獨立的重組產物，
兩者互補但不重疊。
-->

# Brainstorm: subagent-tool-debt-closure

## 背景

经代码级深度审查（对比 opencode /home/crochee/workspace/opencode 与 codex /home/crochee/workspace/codex），synthia 在 Subagent 框架与 Tool 系统存在 8 项真实债。本变更关闭这些债务，不引入新抽象。

审查覆盖 9+11 个维度，三方对抗性分析（性能/可靠性/架构三视角）后过滤掉过度工程项（F17 ToolLifecycleContributor / F10 team 框架接通），保留 8 项必须修复的真实债。

## 决策链

### Q1: 哪些是"归档了但代码没真正用上"的债？

经 grep + 代码路径验证，8 项真实债：

- **F6/F14**: `SubagentManager::current_depth()` 是 stub 返回 0（[team.rs:106-108](file:///home/crochee/workspace/synthia/crates/synthia-agent/src/subagent/team.rs)），`max_depth=3` 配置形同虚设
- **F7**: background 子 agent 完成后结果被 `unwrap_or_else` 丢弃（[agent_tool.rs:244-282](file:///home/crochee/workspace/synthia/crates/synthia-agent/src/tools/agent_tools/agent_tool.rs)），不注入父上下文
- **F8**: 配额 `release_slot()` 在 6 处手动调用（[agent_tool.rs:191,199,232,251,276,294](file:///home/crochee/workspace/synthia/crates/synthia-agent/src/tools/agent_tools/agent_tool.rs)），无 RAII，易泄漏
- **F11**: 无递归子树取消，取消父 agent 时子 agent 可能泄漏
- **F15**: `ToolAdapter::execute` 无输入 schema 校验（[tool-orchestrator/lib.rs:143-164](file:///home/crochee/workspace/synthia/crates/synthia-tool-orchestrator/src/lib.rs)），LLM 坏数据可能 panic
- **F19**: `PermissionChecker` 无 always 持久化，用户每次都要重新确认
- **F20**: 无 `failInterruptedTools` 显式清理，中断后可能僵尸状态
- **F23**: bash `MAX_OUTPUT_BYTES = 30_000` 偏低（opencode/codex 都是 1MB）

### Q2: F8 配额 RAII 化 vs 保留手动 release？

**专家 A（性能）**：RAII 跨 await 点 drop 时机不确定。
**专家 B（可靠性）**：必须 RAII，6 处已易漏，未来加错误路径更危险。
**专家 C（架构）**：用 `SlotGuard { manager, released: bool }` + `commit()`，Drop 时 if !released { release_slot() }。Rust 惯用法，~60 行。

**决策**：采用 RAII。`try_acquire_slot()` 返回 `Option<SlotGuard>`，guard 持有 manager Arc 引用，Drop 自动释放。`commit()` 标记 released=true 防止 double-release。

### Q3: F15 schema 校验如何实现（不引入 Effect 框架）？

**专家 C**：用 `serde_json::from_value::<T>(request.arguments)`，失败返回 `ToolOutput::error("Invalid input: ...")`。但问题是每个工具的 Input 类型不同，`ToolAdapter` 是泛型 `ToolAdapter<T: Tool>`，T::Input 已知。

**决策**：在 `ToolAdapter::execute` 中，要求 `T::Input: DeserializeOwned`，调用 `serde_json::from_value::<T::Input>(request.arguments.clone())`，失败转 `ToolOutput::error`。这是 trait bound 增强，不是新抽象。

### Q4: F6/F14 max_depth 如何接通？

**专家 B**：子 agent 创建时 depth+1，超 max_depth 返回错误。
**专家 C**：但 `derive_subagent_permission` 默认 forced Deny task 已隐式阻止递归。max_depth 是双保险——builtin config 显式 `allow_task: true` 时才生效。

**决策**：`SubagentConfig` 增加 `depth: usize` 字段，`SubagentSessionFactory::create_child` 接受 `parent_depth` 参数，子 config depth = parent_depth + 1。`AgentTool::call` 在 spawn 前检查 `depth >= max_depth` 返回 `ToolOutput::error("Max sub-agent depth reached")`。`current_depth()` 从 stub 改为读 config.depth。

### Q5: F7 background 通知的最小修复 vs 完整修复？

**专家 C**：最小修复（事件通知）P1，完整修复（注入父上下文）P2。

**决策**：本变更做最小修复——background 子 agent 完成时，通过 `parent_event_sender` 发 `AgentEvent::SubagentEvent { inner: SubagentCompleted { session_id, result_summary } }` 到父流。父 agent 下一轮 LLM 能看到。完整注入父 session input queue 留 P2。

### Q6: F11 递归子树取消如何实现？

**决策**：`SubagentManager` 增加 `child_sessions: DashMap<SessionId, CancellationToken>`，`create_child` 时注册子 session_id + child_token（从 parent cancel_token 派生）。`cancel_session_tree(session_id)` 遍历所有 child，cancel child_token，递归 cancel 孙子。这是显式递归取消，不依赖共享 token。

注意：现有 `cancel_token: CancellationToken` 是共享的（clone 传播），但共享 token 无法做"取消父但不取消子"或"取消子但不取消父"的精细控制。本变更引入 per-session child_token，与共享 token 并存。

### Q7: F19 always 持久化的并发安全？

**专家 C**：`saved_rules: DashSet<(String, String)>` 存 (action, resource) 对。`evaluate()` 先查 saved 再查 policy。`always` 回复时 `saved_rules.insert(...)`，并级联批准同 session pending 请求。

**决策**：`PermissionChecker` 增加 `saved_rules: Arc<DashSet<(String, String)>>`。`check()` 中对每个 request，先查 `saved_rules.contains(&(action, resource))`，命中则 AutoApprove。新增 `remember_always(action, resource)` API 供 approval service 调用。

### Q8: F20 failInterruptedTools 的触发时机？

**决策**：在 agent 主循环检测到中断（cancel_token.cancelled() 或 steering 中断）时，遍历 `active_calls: DashMap<String, CancellationToken>`，对每个 pending/running 的 tool call：
1. 调用 `entry.value().cancel()`
2. 从 map 移除
3. 发 `AgentEvent::ToolCallCompleted { tool_name, output: "Tool execution interrupted", is_error: true }`

这与现有 `cancel(call_id)` 单点取消的区别：failInterruptedTools 是**批量清理**，确保中断后无僵尸。

### Q9: F23 bash 输出上限提升到多少？

**决策**：`MAX_OUTPUT_BYTES` 从 30_000 提升到 1_048_576（1MB），对齐 opencode/codex。head+tail 截断逻辑不变（已有 UTF-8 安全边界检查）。

## 设计取捨

### 不做的项（YAGNI / 已有覆盖）

- **F10 team/coordinator 接通**：多 agent 协作非 synthia 场景（SaaS + SDK），prior memory 已明确"全量重写为 opencode 风格 Event Sourcing 不做"。倾向 P2 评估后删除 dead framework。
- **F17 ToolLifecycleContributor**：synthia 已有 `hook_registry`（before_tool/after_tool）覆盖 telemetry/review 需求，引入新抽象是重复。
- **F9 task_id 恢复**：工程量大，需改 session input queue + 持久化层，并入 `p0-subagent-execution-session-persistence` (0/43) OpenSpec change。
- **F21 沙箱升级重试**：并入 `production-tool-execution-sandbox` (6/43) OpenSpec change。

### 优先级裁决

- **P0（立即修复）**：F8 配额 RAII（防泄漏）+ F15 schema 校验（防 panic）
- **P1（本季度）**：F6/F14 max_depth + F7 background 通知 + F11 递归取消 + F19 always 持久化 + F20 failInterruptedTools + F23 bash 上限

### 借鉴来源映射

- F8 RAII → codex `SpawnReservation::Drop` ([agent/registry.rs:345-354](file:///home/crochee/workspace/codex/codex-rs/core/src/agent/registry.rs))
- F15 schema 校验 → opencode `Schema.decodeUnknownEffect`（用 Rust serde 等价）
- F7 background 通知 → opencode `inject("completed"|"error", text)`（最小版）
- F11 递归取消 → codex `list_live_agent_subtree_thread_ids` DFS
- F19 always 持久化 → opencode `saved.add(...)` + 级联批准
- F20 failInterruptedTools → opencode `failInterruptedTools` ([runner/llm.ts:115-135](file:///home/crochee/workspace/opencode/packages/core/src/session/runner/llm.ts))
- F23 bash 上限 → opencode `MAX_CAPTURE_BYTES = 1MB` / codex `EXEC_OUTPUT_MAX_BYTES = 1MB`
