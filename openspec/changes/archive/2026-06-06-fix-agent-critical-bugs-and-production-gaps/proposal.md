## Why

synthia-agent 当前存在多个 production-critical bugs 和架构问题：

1. **Hook Modify 完全失效** - 任何试图修改工具输入的 hook 都无效，安全性无法保障
2. **工具名称丢失** - 所有工具显示为 `tool_0`, `tool_1`，错误追踪和 memory 引用完全失效
3. **Token 追踪未连线** - `TokenBudgetWarning` 事件发送硬编码 0 值，无法触发正确的 compaction
4. **~1500 行死代码** - 三套 Agent 实现、两套 reflection 逻辑，增加维护负担和认知成本

这些问题影响生产稳定性（panic 风险）、可观测性（错误追踪失效）、可维护性（重复代码）。

## What Changes

**Hook Modify 修复**
- From: hook 返回 `Modify` 但实际执行仍用原始参数
- To: 修改后的参数实际用于工具执行
- Impact: Non-breaking，hook 行为更符合预期

**工具名称保留**
- From: 工具结果使用 `tool_{index}` 作为名称
- To: 使用原始工具定义中的真实名称
- Impact: Non-breaking，提升可调试性

**Unsafe unwrap 修复**
- From: `ctx.end_reason.clone().unwrap()` 可能 panic
- To: 使用 `unwrap_or_else` 提供默认值
- Impact: Non-breaking，消除 panic 风险

**Token 追踪实现**
- From: `TokenBudgetWarning` 发送 `current_tokens: 0, threshold_tokens: 0`
- To: 发送实际累计 token 数和配置阈值
- Impact: Non-breaking，使 compaction 机制正常工作

**Silent Error Swallowing 改进**
- From: 6+ 处 `let _ =` 静默忽略错误
- To: 结构化日志记录警告
- Impact: Non-breaking，提升可观测性

**死代码清理**
- 删除 `agent_runtime.rs`、`agent.rs`、冗余的 `step_self_reflection()`
- Impact: Non-breaking，需验证无外部依赖

## Capabilities

### New Capabilities

- `hook-modify-tool-input`: Hook 可以实际修改工具输入参数并生效
- `token-budget-observability`: TokenBudgetWarning 事件发送实际 token 计数
- `structured-error-logging`: 关键路径错误被结构化记录而非静默吞噬

### Modified Capabilities

- `tool-execution-result`: ToolResult.tool_name 从伪名改为真实工具名

## Impact

**受影响代码**:
- `crates/synthia-agent/src/stream_builder/builder.rs`（Hook Modify, error swallowing）
- `crates/synthia-agent/src/stream_builder/steps/tool_execute.rs`（工具名称）
- `crates/synthia-agent/src/stream_builder/steps/sample.rs`（token 追踪）
- `crates/synthia-agent/src/react.rs`（死代码删除）

**测试影响**:
- 需要添加 Hook Modify 集成测试
- 现有测试应继续通过

**无 API 变更**: 所有修改都是内部实现，不影响 public API