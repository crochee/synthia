## ADDED Requirements

### Requirement: AgentHandle struct
AgentHandle 是无状态推理句柄，可跨 N 个 AgentSession 复用。
- 持有: id, config, provider, tool_registry, hook_registry, context_assembler, model_router, interceptor_chain, a2a_card
- 不持有: session, loop_state, history, token_budget, compaction_state
- 实现 Clone（共享 Arc 字段）
- 实现 RegistryItem（name = id, description = config.system_prompt）

### Requirement: AgentSession struct
AgentSession 是私有会话状态，每次运行独立。
- 持有: id, agent_id, history, token_budget, loop_state, compaction_state
- agent_id 反指所属 AgentHandle
- 提供 push_message(), get_history(), compact() 方法

### Requirement: AgentInstance deprecation
AgentInstance 保留为 `type AgentInstance = AgentHandle` 的 type alias（Phase 1 过渡），Phase 6 删除。
所有现有 AgentInstance 的使用点迁移到 AgentHandle + AgentSession。

### Requirement: AgentRunConfig simplification
AgentRunConfig 不再重复携带 tool_registry / hook_registry / session_store。
这些从 AgentHandle 获取。AgentRunConfig 只保留运行时参数（session_id, user_id, cancel_token 等）。
