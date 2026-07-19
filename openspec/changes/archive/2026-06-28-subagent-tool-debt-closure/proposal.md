## Why

经代码级深度审查（对比 opencode 与 codex），synthia 在 Subagent 框架与 Tool 系统存在 8 项真实债：配额手动 release 易泄漏（F8）、工具无输入 schema 校验可能 panic（F15）、max_depth 配置形同虚设（F6/F14）、background 结果丢弃（F7）、无递归子树取消（F11）、无 always 权限持久化（F19）、无 failInterruptedTools 显式清理（F20）、bash 输出 30KB 上限偏低（F23）。这些债影响安全防线、可靠性和用户体验，需立即关闭，不引入新抽象。

## What Changes

**配额管理 RAII 化（F8）**
- From: `try_acquire_slot()` 返回 `bool`，调用方在 6 处手动 `release_slot()`，易泄漏
- To: `try_acquire_slot()` 返回 `Option<SlotGuard>`，Drop 自动释放，`commit()` 标记已完成
- Reason: 防 quota 泄漏（Rust 惯例，借鉴 codex `SpawnReservation::Drop`）
- Impact: non-breaking，调用方改用 guard

**工具输入 schema 校验（F15）**
- From: `ToolAdapter::execute` 直接传 `request.arguments` 给工具，无校验
- To: 要求 `T::Input: DeserializeOwned`，`serde_json::from_value` 失败转 `ToolOutput::error`
- Reason: 防 LLM 坏数据导致 panic
- Impact: non-breaking，trait bound 增强

**max_depth 接通（F6/F14）**
- From: `current_depth()` 是 stub 返回 0，`max_depth=3` 配置永不触发
- To: `SubagentConfig` 增加 `depth` 字段，spawn 时 +1，超限返回 error
- Reason: 安全防线接通，防递归失控
- Impact: non-breaking，stub 改为真实实现

**background 最小通知（F7）**
- From: background 子 agent 完成后结果被 `unwrap_or_else` 丢弃
- To: 完成时发 `SubagentCompleted` 事件到父流，父 LLM 下一轮可见
- Reason: background 模式从半成品变可用
- Impact: non-breaking，新增事件

**递归子树取消（F11）**
- From: 取消父 agent 时子 agent 可能泄漏（依赖共享 token，无显式递归）
- To: `SubagentManager` 增加 `child_sessions: DashMap`，`cancel_session_tree` 递归取消
- Reason: 防 zombie 子 agent
- Impact: non-breaking，新增 API

**always 权限持久化（F19）**
- From: `PermissionChecker` 每次重新评估，用户重复确认
- To: 增加 `saved_rules: DashSet<(action, resource)>`，命中则 AutoApprove，新增 `remember_always` API
- Reason: 用户体验痛点
- Impact: non-breaking，新增字段和 API

**failInterruptedTools 显式清理（F20）**
- From: 中断时依赖 CancellationToken，无显式遍历 pending 工具发 Failed 事件
- To: 中断时遍历 `active_calls`，批量 cancel + 发 `ToolCallCompleted { is_error: true }`
- Reason: 防僵尸状态，状态一致性
- Impact: non-breaking，新增批量清理

**bash 输出上限提升（F23）**
- From: `MAX_OUTPUT_BYTES = 30_000`
- To: `MAX_OUTPUT_BYTES = 1_048_576`（1MB，对齐 opencode/codex）
- Reason: LLM 看到完整错误上下文
- Impact: non-breaking，常量调整

## Capabilities

### New Capabilities

- `subagent-quota-raii`: 配额管理 RAII 化，SlotGuard Drop 自动释放，防泄漏
- `tool-input-validation`: 工具输入 serde schema 校验，LLM 坏数据优雅降级
- `subagent-tree-cancellation`: 递归子树取消，cancel_session_tree 显式 DFS
- `permission-always-persist`: always 权限持久化，saved_rules 命中自动批准
- `tool-interrupt-cleanup`: failInterruptedTools 批量清理中断时的 pending 工具

### Modified Capabilities

- `subagent-background-mode`: 增加 background 完成时发 SubagentCompleted 事件到父流
- `subagent-session-model`: SubagentConfig 增加 depth 字段，current_depth 从 stub 改为真实实现
- `bash-utf8-safe-truncate`: MAX_OUTPUT_BYTES 从 30KB 提升到 1MB

## Impact

**受影响 crate**:
- `synthia-agent`（subagent/team.rs, subagent/config.rs, subagent/factory.rs, tools/agent_tools/agent_tool.rs, stream_builder/builder/tool_execution/execute.rs, tools/builtins/system_tools.rs）
- `synthia-tool-orchestrator`（lib.rs ToolAdapter::execute）
- `synthia-permission`（checker/checker.rs）
- `synthia-tool-bash`（如有常量引用）

**受影响 API**:
- `SubagentManager::try_acquire_slot` 返回类型变更（bool → Option<SlotGuard>）
- `SubagentManager::current_depth` 实现变更（stub → 真实）
- `SubagentSessionFactory::create_child` 签名增加 parent_depth 参数
- `PermissionChecker::check` 内部逻辑变更（先查 saved_rules）
- `PermissionChecker::remember_always` 新增 API

**依赖**: 无新依赖（使用现有 serde_json / dashmap / tokio_util::sync::CancellationToken）

**测试**: 每项变更需新增单元测试，覆盖 happy path + 边界（depth 超限 / schema 失败 / guard drop 释放 / saved_rules 命中 / 批量清理）
