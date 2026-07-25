## ADDED Requirements

### Requirement: transfer_bidirectional function
transfer_bidirectional(agent_a, agent_b_url, transport) 语义：
- agent_a.tool_registry 注册 SendMessageTool(for agent_b_url)
- agent_a.tool_registry 注册 SendMessageStreamTool(for agent_b_url)
- agent_b 端由自己启动时配置对 a 的引用（对称）

### Requirement: HandoffTool removal
HandoffTool 删除，替代为 SendMessageTool（A2A）。
HandoffTool 是单向消息（fire-and-forget），SendMessage 支持完整的 Task 生命周期。
