## Why

Synthia 已有完整的 L1-L5 错误恢复实现（5 个 `error_recovery/*` 模块，13 个单元测试通过），但 `stream_builder/builder.rs` 从不调用 `run_recovery_cascade`。结果是：5 层恢复的代码是"已实现的死代码" — 工具错误不触发任何恢复，LLM 错误只触发 L2 retry 然后 session 直接终止。

上轮 `error-recovery-cascade` change 的 retrospective 明确指出此 gap（"builder.rs 的实际端到端 wiring 仍有小幅差距"），并写了 5 个 specs 描述应有的恢复行为，但实现与 spec 脱节。本 change 把 cascade 显式 wire up 到两个错误入口，添加 `AgentEvent::RecoveryApplied` 提供可观察性，让 spec 与实现对齐。

## What Changes

**LLM 错误处理路径**
- From: `handle_error(L2Retry)` 后 `Escalated` 分支直接 `yield SessionEnded + return`，跳过 L3-L5
- To: 改为调 `run_recovery_cascade`，L3-L5 真正生效；L5 reset 成功时 `ctx.messages` 被清空，session 可继续
- Reason: 错误应优先尝试恢复，不应直接终止 session
- Impact: 非破坏性（仅在错误路径生效）

**工具错误处理路径**
- From: `Err(e)` 转换为 `is_error: true` 的 ToolResult 推入上下文，LLM 看到错误后自求多福
- To: 调 `run_recovery_cascade`；L3 fallback 消息注入为 tool result（`output = "Describing the command instead of executing"`），LLM 收到 fallback 提示继续
- Reason: 工具错误有可恢复路径（web_fetch 退到缓存、bash 退到描述），不应污染上下文
- Impact: 非破坏性（fallback 消息更友好，但 LLM 仍能收到错误信号）

**Tool result L1 truncate**
- From: `tool_results` 注入 context 前**不**做 truncate，超大输出（>30KB）直接进 LLM
- To: 每个 tool result 注入前调 `truncate_output`，超长则 truncate 并 yield `RecoveryApplied { level_number: 1 }`
- Reason: 上轮 `specs/tool-output-truncate` 已写明该行为，但未实现
- Impact: 行为变更（tool result 可能被截断），但 LLM 始终收到完整信号（"truncated" marker）

**AgentEvent 新增变体**
- From: `AgentEvent` 无 `RecoveryApplied` 变体，外部无法感知恢复触发
- To: 新增 `RecoveryApplied { level_number, tool_name, message, iteration }`
- Reason: observability — 调试 / 监控 / 教学时需要知道"为什么没崩"
- Impact: 非破坏性（新变体，下游消费者加 match 分支即可）

**BuilderSteps 字段扩展**
- From: `recovery: ErrorRecoveryCoordinator` 一个字段
- To: + `reset: ResetCoordinator` + `failure_tracker: ConsecutiveFailureTracker`
- Reason: cascade 内部需要这些 mutable state 跨调用持续
- Impact: 内部字段，对外 API 不变

## Capabilities

### New Capabilities

- `recovery-cascade-wiring`: 显式化 L1-L5 cascade 在 builder.rs 的两个错误入口（LLM 错误 + 工具错误）的调用；包括 L1 tool result truncate 和 L3-L5 cascade 触发 + `AgentEvent::RecoveryApplied` 事件 yield

### Modified Capabilities

（无 — 上轮 archive 的 5 个 specs 不修改，本 change 只 wire up 它们描述的行为）

## Impact

- **代码**：
  - 修改 `crates/synthia-agent/src/stream_builder/builder.rs`（wire up cascade，~80 行变更）
  - 修改 `crates/synthia-agent/src/events.rs`（新增 `AgentEvent::RecoveryApplied` 变体，~10 行）
  - 修改 `crates/synthia-agent/src/config.rs`（`AgentRunConfig` 可能新增 `compaction_provider` 字段，~5 行）
  - 测试文件：新增 3+ integration test，~150 行
- **API**：
  - `AgentEvent` 新增变体（公开 enum）— 下游消费者需更新 match 分支
  - `BuilderSteps` 新增 2 个字段（pub，但仅在 `builder.rs` 内部使用，外部不直接构造）
- **依赖**：无新增外部依赖
- **Spec 验证**：5 个 archive specs (`auto-compact-on-error`, `session-reset`, `tool-fallback`, `tool-output-truncate`, `tool-retry`) 继续通过
- **测试**：3+ 新增 integration test，预期 1500+ 总测试数
