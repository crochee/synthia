## ADDED Requirements

### Requirement: synthia-a2a crate
新建 synthia-a2a crate，依赖 a2a-lf / a2a-client-lf / a2a-server-lf。
提供 A2aTransport, SynthiaA2aHandler, AgentCard 构建。

### Requirement: A2aTransport struct
A2aTransport 持有:
- server: Option<A2aServer> — 暴露此 agent 给其他 agent 调用
- client_registry: DashMap<String, A2aClient> — 发现的远程 agent client 缓存
- card: AgentCard — 此 agent 的能力名片

### Requirement: A2aTransport.from_handle
从 AgentHandle 构建 A2aTransport：
- AgentCard.name = handle.id
- AgentCard.skills = handle.tool_registry 的工具列表
- AgentCard.capabilities.streaming = true

### Requirement: A2aTransport.serve
启动 A2A Server，其他 agent 可通过 A2A 协议发现和调用此 agent。
SynthiaA2aHandler 桥接 on_send_message → handle.run, on_send_streaming_message → handle.run_stream。

### Requirement: A2aTransport.discover
发现远程 agent：GET /.well-known/agent.json → 缓存 A2aClient。

### Requirement: InMemoryMessageBus removal
InMemoryMessageBus 和 MessageBus trait 删除。所有 agent 间通信走 A2A 协议。
