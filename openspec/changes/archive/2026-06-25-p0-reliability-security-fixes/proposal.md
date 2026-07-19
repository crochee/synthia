## Why

多专家对抗性审查发现 5 个 P0 级确定性风险：bash 超时不杀进程组导致孤儿进程泄漏、L5 ResetCoordinator 半实现导致 6+错误进入冷却死循环、无全局 wall-clock 超时导致失控 session 可跑 20 小时、Guardian 快速路径空 transcript 导致跨轮次 prompt injection 防护失效、Guardian 占位符 request 导致用户确认指向错误动作。这些是确定性风险（非概率性），在生产中必然发生，必须立即修复。

## What Changes

**bash 进程组杀**
- From: 超时/取消时 future 被 drop，子进程不被 kill，孙子进程变孤儿
- To: 使用 `process_group(0)` 创建新进程组，超时时 `killpg(SIGTERM)` → 3s → `killpg(SIGKILL)`，IO 排空 2s
- Reason: 消除确定性资源泄漏
- Impact: non-breaking，bash 工具行为更安全

**L5 Reset 回退**
- From: `determine_scope` 在 ToolState/Full 未实现时返回错误，触发 30s 冷却
- To: 未实现的 scope 回退到 Conversation，添加 warning 日志
- Reason: 消除 6+错误冷却死循环
- Impact: non-breaking，错误恢复更可靠

**全局 wall-clock 超时**
- From: `should_stop` 仅检查 `max_iterations`，无时间限制
- To: 增加 `session_wall_clock_timeout`（默认 30 分钟），在 `should_stop` 中检查
- Reason: 防失控 session 无限占用资源
- Impact: non-breaking，可通过配置禁用

**Guardian 空 transcript 修复**
- From: `check()` 快速路径传入 `&[]`，无对话上下文
- To: `check()` 接收 `conversation: &[Message]` 参数，传入最近 N 轮（有 token 预算）
- Reason: 修复跨轮次 prompt injection 防护失效
- Impact: breaking（签名变更），需更新所有调用方

**Guardian 占位符 request 修复**
- From: `make_guardian_decision` 用 `ApprovalRequest::shell("temp", ...)` 占位符
- To: 接收并透传实际 `request` 参数
- Reason: 修复用户确认指向错误动作
- Impact: breaking（签名变更），需更新调用方

## Capabilities

### New Capabilities
- `process-lifecycle`: bash 工具的进程组创建、超时杀进程组、IO 排空
- `session-timeout`: session 级 wall-clock 超时检查与配置

### Modified Capabilities
- `error-recovery`: L5 ResetCoordinator 未实现 scope 的回退行为
- `guardian-review`: Guardian `check()` 的 conversation 上下文与 request 透传

## Impact

**受影响代码**：
- `crates/synthia-agent/src/tools/builtins/system_tools.rs` — bash 进程组杀
- `crates/synthia-agent/src/error_recovery/reset/coordinator.rs` — L5 回退
- `crates/synthia-agent/src/loop_context.rs` — 全局超时
- `crates/synthia-agent/src/agent.rs` — AgentConfig 增加 timeout 配置
- `crates/synthia-guardian/src/review/reviewer.rs` — Guardian 两个 bug 修复
- 所有调用 `GuardianReviewer::check()` 的位置 — 签名变更

**依赖**：`nix` crate（Unix 信号处理）或 `tokio::signal::unix`

**测试**：每个修复需单元测试；进程组杀需集成测试验证孤儿进程清理
