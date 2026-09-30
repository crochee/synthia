## Why

Synthia 的 AgentTool 能创建子智能体实例但从不执行 — 返回 "Waiting for result..." 后无任何实际工作。同时，会话状态在进程重启后丢失关键信息（结束原因、累计 token 数、转向消息），导致无法正确恢复执行。这两个问题使 Synthia 无法作为生产级 agent 运行。OpenCode 和 Codex 均已实现完整的子智能体执行和持久会话状态，本次变更旨在弥合这些差距。

## What Changes

**Sub-Agent Execution Loop**
- From: `AgentTool::call()` 创建 `AgentInstance` 后立即返回占位文本 "Waiting for result..."，无实际执行
- To: `AgentTool::call()` 启动子智能体的 ReAct 执行循环，支持前台阻塞等待（`background: false`）和后台 fire-and-forget（`background: true`）两种模式
- Reason: 子智能体是生产级 agent 的核心能力（OpenCode 的 task 工具、Codex 的 spawn_agent），Synthia 的缺失是 P0 级别的功能差距
- Impact: 非破坏性变更 — `AgentTool` 的 API 向后兼容，新增 `background` 参数有默认值

**AgentInstance Type Unification**
- From: 两套并行的 `AgentInstance` 类型（`registry::instance::AgentInstance` 和 `tools::agent_tools::coordinator::AgentInstance`），各自不完整
- To: 单一 `AgentInstance` 类型，包含执行所需全部字段
- Reason: 消除类型歧义，减少维护成本，为子智能体执行提供统一的数据模型
- Impact: 非破坏性变更 — 旧路径保留为 `pub use` shim

**Session State Persistence**
- From: `LoopContext` 的 `end_reason`、`cumulative_tokens`、`context_token_limit` 等字段仅内存，转向消息通过内存 mpsc channel 传递
- To: 扩展 `SessionMetadata` 持久化关键回放字段，新增 `SessionInputQueue` 将转向消息持久化到 `session_input.jsonl`
- Reason: 进程重启后 agent 应能正确恢复执行状态，不应丢失转向消息
- Impact: 非破坏性变更 — 所有新增字段使用 `serde(default)` 保证向后兼容

## Capabilities

### New Capabilities
- `subagent-execution`: 子智能体的完整执行循环 — spawn、配置继承、前台/后台模式、结果收集、深度/并发限制
- `session-state-persistence`: 会话关键状态持久化 — LoopContext 回放字段、转向消息队列、恢复路径增强

### Modified Capabilities
<!-- 无现有 spec 需要修改 -->

## Impact

**Affected crates:**
- `synthia-agent` — 重写 `AgentTool::call()`、统一 AgentInstance 类型、接线 AgentControl、实现 run_subagent()
- `synthia-session` — 扩展 `SessionMetadata`、新增 `SessionInputQueue`
- `synthia-agent/src/stream_builder/` — 主循环使用 AgentControl、从 SessionInput 读取转向
- `synthia-agent/src/loop_context.rs` — 从 Metadata 恢复字段

**Non-breaking changes only.** 所有新增字段有默认值，旧类型路径保留为 shim。