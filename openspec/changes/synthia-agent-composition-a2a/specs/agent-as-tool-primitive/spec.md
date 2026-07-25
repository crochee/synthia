## ADDED Requirements

### Requirement: agent_as_tool pure function
`pub fn agent_as_tool(handle: Arc<AgentHandle>) -> AgentTool` 是纯转换函数，无副作用。
将 AgentHandle 包成 Tool impl，name = handle.id, description = handle.config.system_prompt。

### Requirement: AgentTool call semantics
AgentTool::call() 语义：创建新 AgentSession → handle.run(session, prompt) → 返回 ToolOutput。
每次调用创建独立 Session，不共享状态。

### Requirement: AgentTool parameter schema
parameters: { "prompt": string (required), "context": string (optional) }
prompt 是发给 agent 的任务描述，context 是附加上下文。

### Requirement: SubagentManager removal
SubagentManager 删除。其职责分散：
- depth 限制 → ToolOrchestrator 执行策略
- 并发控制 → ToolOrchestrator 已有
- 子树取消 → CancellationToken 层级
- parent_config → AgentHandle 已包含
- session 注册/注销 → AgentSession 自管理
SlotGuard 同时删除。
