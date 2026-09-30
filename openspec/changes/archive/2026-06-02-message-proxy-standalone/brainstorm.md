<!--
Raw capture of superpowers:brainstorming output.

本檔原樣捕捉 brainstorming skill 的產出，不強制結構。
Skill 的自然產出通常是 decision log 格式（背景 → 決議鏈 Q1-Qn → 設計取捨），
但依對話內容可能有不同組織方式。

design.md 從本檔萃取並重新整理為結構化設計文件。

不要將本檔的內容複製到 design.md — design.md 是獨立的重組產物，
兩者互補但不重疊。
-->

# Brainstorming: Multi-agent Message Proxy Standalone

## 背景

当前 `synthia-agent` 的 multi-agent 协调使用 `InMemoryMessageBus` (DashMap + mpsc channels)，存在以下问题：
- 进程重启后消息丢失
- 无 dead letter queue
- 无消息持久化
- 不支持真正的跨进程协调

## 决策链

### Q1: MessageProxy 部署形态

**选项**:
- A: 独立进程 (推荐) - Agent crash 不影响消息路由
- B: 嵌入式 (同一进程内)

**决策**: A (独立进程)

```
┌─────────────┐     Unix Socket     ┌─────────────┐
│  Agent A    │◄──────────────────►│             │
└─────────────┘                     │  Message    │
                                    │  Proxy      │◄── Agent B
┌─────────────┐     gRPC/TCP        │  (独立进程)  │
│  Agent C    │◄──────────────────►│             │
└─────────────┘                     └─────────────┘
```

### Q2: 消息投递语义

**选项**:
- A: At-Least-Once + DLQ
- B: At-Most-Once (Fire-and-Forget)
- C: At-Least-Once 无 DLQ (同步等待)

**决策**: B (At-Most-Once)

- 最低延迟
- 消息不持久化
- Agent 不可达则丢弃

### Q3: 消息模式

**选项**:
- A: Point-to-Point (1:1)
- B: Point-to-Point + Broadcast (1:N)
- C: Request-Response

**决策**: B (Point-to-Point + Broadcast)

- Direct message: Agent A → Agent B
- Broadcast: Agent A → [Agent B, Agent C, Agent D]

### Q4: Agent 如何发现和连接 Proxy

**选项**:
- A: Unix Domain Socket
- B: TCP (127.0.0.1:端口)
- C: 环境变量配置 + 默认值

**决策**: C (环境变量配置，有默认值)

- 环境变量: `MESSAGE_PROXY_ADDR`
- 默认值: `/var/run/synthia/message-proxy.sock` (Unix Domain Socket)
- 灵活，可在测试时切换 mock

### Q5: Agent 与 Proxy 之间的认证

**选项**:
- A: 无认证 (开发模式)
- B: Token 认证
- C: mTLS

**决策**: A (无认证)

- 同一机器上，风险可控
- 开发模式优先

## 最终设计决策

| 维度 | 决策 |
|------|------|
| 部署形态 | 独立进程 (MessageProxy 独立运行) |
| 传输协议 | gRPC over Unix Domain Socket |
| 连接配置 | 环境变量 `MESSAGE_PROXY_ADDR`，默认 `/var/run/synthia/message-proxy.sock` |
| 消息模式 | Point-to-Point (1:1) + Broadcast (1:N) |
| 投递语义 | At-Most-Once，Agent 不可达则丢弃 |
| 认证 | 无 |

## Proto 定义

```protobuf
service MessageProxy {
  rpc Send(Message) returns (SendResult);
  rpc Broadcast(BroadcastRequest) returns (BroadcastResult);
  rpc Register(RegisterRequest) returns (RegisterResponse);
  rpc Subscribe(SubscribeRequest) returns (stream Message);
}

message Message {
  string id = 1;
  string from = 2;
  string to = 3;           // 空则广播
  bytes payload = 4;
  int64 timestamp = 5;
}

message BroadcastRequest {
  string from = 1;
  repeated string recipients = 2;  // 空则发给所有
  bytes payload = 3;
}
```

## 用户确认

✓ 符合预期
