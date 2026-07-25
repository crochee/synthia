## ADDED Requirements

### Requirement: SendMessageTool
SendMessageTool 实现 Tool trait：
- name = "SendMessage"
- description = "Send a message to a remote agent via A2A protocol and wait for response"
- parameters: { agent_url: string (required), message: string (required), metadata: object (optional) }
- call(): A2aClient.send_message() → 等待 Task 完成 → 从 Artifact 提取结果

### Requirement: SendMessageStreamTool
SendMessageStreamTool 实现 Tool trait：
- name = "SendMessageStream"
- description = "Send a message to a remote agent via A2A and receive streaming response"
- parameters: 同 SendMessageTool
- call(): A2aClient.send_streaming_message() → 收集 StreamEvent → 拼接最终结果

### Requirement: A2A tool registration
AgentHandle 初始化时，如果配置了远程 agent URL，自动注册 SendMessageTool 和 SendMessageStreamTool 到 tool_registry。
