## Why

Synthia 当前的 Agent 架构混合了能力与状态（AgentInstance 同时持有 definition 和 session），SubagentManager 是独立概念而非 agent-as-tool 的自然推论，Multi-Agent 模式层缺失（无 Generator-Verifier / Workflow / 对称转移），且横切关注点（审批、重试、trace）分散在不同子系统中。基于"Agent-as-Tool 为唯一原语、组合优先"的设计原则，结合 A2A 协议实现跨 agent 标准通信，从根本上精简架构并释放组合能力。

## What Changes

**From** AgentInstance 混合能力(definition)和状态(session)，SubagentManager 独立管理子 agent 生命周期，InMemoryMessageBus 做内部消息传递，HandoffTool 做单向转移，审批/重试/trace 走独立路径。

**To** AgentHandle（无状态句柄）+ AgentSession（私有状态）正交分离，agent_as_tool() 纯函数将 agent 包成 Tool，A2A 协议（a2a-lf）实现跨 agent 标准通信，SendMessage/SendMessageStream Tool 覆盖本地+远程场景，Generator-Verifier / Workflow / Transfer 从 agent_as_tool() 自然组合，Interceptor Chain 统一横切关注点。

## Capabilities

### New Capabilities

- **agent-handle-session-separation**: AgentHandle（无状态推理句柄，可跨 Session 复用）与 AgentSession（私有状态）正交分离，消除 AgentInstance 的概念混合
- **agent-as-tool-primitive**: agent_as_tool() 纯函数将 AgentHandle 包成 Tool，作为 Multi-Agent 的唯一原语，替代 SubagentManager + AgentTool(task)
- **a2a-transport**: A2A 协议通信层（a2a-lf），每个 Agent 可同时作为 A2A Client/Server，支持本地 in-process 和远程 HTTP/gRPC
- **send-message-tools**: SendMessage Tool（A2A 同步通信）和 SendMessageStream Tool（A2A 流式通信），覆盖跨 agent 交互
- **generator-verifier-pattern**: Generator-Verifier 闭环模式（生成→验证→循环直到 PASS），基于 agent_as_tool() 组合
- **workflow-pattern**: Workflow 流水线模式（多 agent 串行 pipe），基于 agent_as_tool() 组合
- **transfer-pattern**: 双向对称转移模式（A↔B），基于 SendMessage A2A 通信
- **interceptor-chain**: Interceptor Chain 统一横切关注点（Trace / Approval / Retry / Compact / LoopDetect），替代分散的 HookBuilder + ApprovalService + EnhancedToolDispatcher
- **agent-executor-trait**: AgentExecutor trait（run / resume）+ AgentStreamExecutor trait（run_stream / resume_stream），统一为两条清晰路径

### Modified Capabilities

- **agent-tool-registry**: ToolRegistry 注册 agent_as_tool() 产出的 Tool，替代原有 SubagentManager 注册路径
- **loop-services**: LoopServices 移除 SubagentManager 依赖，改用 AgentHandle + InterceptorChain

### Removed Capabilities

- **subagent-manager**: 删除 SubagentManager、SlotGuard、InMemoryMessageBus
- **agent-tools-legacy**: 删除 TeamCreateTool、TeamDeleteTool、HandoffTool、SendMessageTool(旧)
- **agent-instance**: 删除 AgentInstance，拆入 AgentHandle + AgentSession
- **hook-builder-deprecated**: 删除 HookBuilder deprecated 方法
- **enhanced-dispatcher-deprecated**: 删除 EnhancedToolDispatcher
