## Why

当前 multi-agent 协调使用内存消息总线 (DashMap + mpsc)，进程重启后消息丢失，无法支持跨进程协作。MessageProxy 作为独立进程提供消息路由，实现真正的进程间通信，为 multi-agent 协作提供可靠基础设施。

## What Changes

**MessageProxy 独立进程**
- From: InMemoryMessageBus (进程内，消息不持久化)
- To: MessageProxy 独立进程，gRPC over Unix Domain Socket
- Reason: 支持跨进程消息路由，提高系统可靠性
- Impact: non-breaking，新增组件

**Point-to-Point + Broadcast 消息模式**
- From: 仅内存队列，无广播能力
- To: 支持 Direct message 和 Broadcast
- Reason: 满足多 agent 协作需求 (任务分发、全局通知)
- Impact: non-breaking，能力扩展

**环境变量配置**
- From: 硬编码连接参数
- To: `MESSAGE_PROXY_ADDR` 环境变量配置
- Reason: 提高灵活性，支持测试环境切换
- Impact: non-breaking

## Capabilities

### New Capabilities

- `message-proxy`: 独立的 gRPC 消息代理服务，支持 Point-to-Point 和 Broadcast 消息路由
- `message-bus-proxy`: Agent 端客户端库，封装与 MessageProxy 的通信
- `agent-registration`: Agent 向 MessageProxy 注册其存在，支持订阅消息

### Modified Capabilities

- `agent-tools`: 修改 AgentTool 等工具，使用 MessageBusProxy 替代 InMemoryMessageBus

## Impact

- **新增 crate**: `synthia-message-proxy` (或放在 `crates/synthia-message-proxy/`)
- **新增 proto**: `message_proxy.proto` 定义 gRPC 服务
- **修改 crate**: `synthia-agent` - 替换 InMemoryMessageBus 为 MessageBusProxy
- **依赖**: `tonic` (gRPC), `prost`, `tokio`
