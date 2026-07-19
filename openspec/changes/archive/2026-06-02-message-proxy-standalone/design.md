## Context

当前 `synthia-agent` 的 multi-agent 协调使用 `InMemoryMessageBus` (DashMap + mpsc channels)，存在以下问题：
- 进程重启后消息丢失
- 无 dead letter queue
- 无消息持久化
- 不支持真正的跨进程协调

MessageProxy 将作为独立进程运行，负责 multi-agent 消息路由，支持 Point-to-Point 和 Broadcast 两种模式。

## Goals / Non-Goals

**Goals:**
- 实现独立的 MessageProxy 进程，通过 gRPC 提供消息路由服务
- 支持 Point-to-Point (1:1) 和 Broadcast (1:N) 消息模式
- 通过 Unix Domain Socket 连接，提供低延迟通信
- 支持环境变量配置连接地址

**Non-Goals:**
- 不实现消息持久化 (At-Most-Once 语义)
- 不实现认证机制 (开发模式)
- 不实现 dead letter queue
- 不支持跨机器分布式部署

## Decisions

### D1：部署形态 - 独立进程

- **选择**：MessageProxy 作为独立进程运行
- **理由**：Agent crash 不影响消息路由，进程隔离便于监控和扩展
- **已考虑 alternative**：
  - 嵌入式 (同一进程内)：简单但 Agent crash 会影响 proxy，不采用

### D2：传输协议 - gRPC over Unix Domain Socket

- **选择**：gRPC over Unix Domain Socket
- **理由**：成熟 RPC 框架，支持流控和重试，Unix Socket 延迟最低
- **已考虑 alternative**：
  - TCP：延迟稍高，需要网络配置，采用
  - 自定义协议：复杂度高，不采用

### D3：连接配置 - 环境变量 + 默认值

- **选择**：`MESSAGE_PROXY_ADDR` 环境变量，默认 `/var/run/synthia/message-proxy.sock`
- **理由**：灵活配置，测试时可切换 mock 实现
- **已考虑 alternative**：
  - 硬编码路径：不灵活，不采用
  - TCP 固定端口：不如 Unix Socket 适合本地开发，不采用

### D4：消息模式 - Point-to-Point + Broadcast

- **选择**：支持 Direct message (1:1) 和 Broadcast (1:N)
- **理由**：满足当前 multi-agent 协作需求
- **已考虑 alternative**：
  - 仅 Point-to-Point：无法支持广播场景，不采用
  - Request-Response：需要 correlation_id 机制，暂不需要

### D5：投递语义 - At-Most-Once

- **选择**：消息不持久化，Agent 不可达则丢弃
- **理由**：最低延迟，实现简单
- **已考虑 alternative**：
  - At-Least-Once + DLQ：增加复杂度，当前场景不需要，不采用
  - 同步等待：可能导致 Agent 阻塞，不采用

## Risks / Trade-offs

- [Risk] MessageProxy 进程崩溃 → 所有 agent 无法通信 → Mitigation: 实现健康检查和自动重启机制
- [Trade-off] At-Most-Once 可能丢消息 → 接受：低延迟优先，非关键消息可接受
- [Risk] Unix Socket 文件系统权限问题 → Mitigation: 启动时创建目录和 socket 文件

## Migration Plan

1. 创建 `message-proxy` crate，包含 proto 定义和 gRPC 服务端实现
2. 实现 `MessageProxyService` gRPC 服务
3. 实现 Agent 端的 `MessageBusProxy` 客户端
4. 替换现有的 `InMemoryMessageBus` 为 `MessageBusProxy`
5. 配置环境变量支持

N/A — 本 change 不涉及部署变更（新增独立服务）

## Open Questions

- 是否需要实现简单的健康检查端点 (用于监控 MessageProxy 状态)？
- Broadcast 时是否需要确认机制？
