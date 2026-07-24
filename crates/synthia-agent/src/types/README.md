# Types 模块

类型定义模块，提供 Agent 核心类型定义。

## 核心组件

| 组件 | 文件 | 功能描述 |
|------|------|----------|
| `AgentEvent` | [event.rs](event.rs) | Agent 事件枚举 |
| `AgentStatus` | [event.rs](event.rs) | Agent 状态枚举 |
| `SystemNotification` | [notification.rs](notification.rs) | 系统通知结构体 |
| `SystemNotificationType` | [notification.rs](notification.rs) | 系统通知类型枚举 |

## AgentStatus 枚举

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum AgentStatus {
    PendingInit,           // Agent 待初始化
    Running,               // Agent 运行中
    Completed,            // Agent 成功完成
    Errored(String),       // Agent 发生错误
    Shutdown,              // Agent 已关闭
    Cancelled,             // Agent 已取消
    MaxStepsReached(u32),  // 达到最大步数
    NotFound,              // Agent 未找到
}
```

## AgentEvent 枚举

```rust
#[derive(Clone, Debug, Serialize)]
pub enum AgentEvent {
    Message(SamplingMessage),                              // 消息
    McpNotification((String, ServerNotification)),         // MCP 通知
    ModelChange { model: String, mode: String },            // 模型变更
    HistoryReplaced(Vec<SamplingMessage>),                 // 历史替换
    SystemNotification(SystemNotification),                 // 系统通知
    Status(AgentStatus),                                    // 状态变更
}
```

## SystemNotificationType 枚举

```rust
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum SystemNotificationType {
    #[default]
    InlineMessage,  // 内联消息
    Progress,       // 进度更新
    Log,            // 日志消息
}
```

## SystemNotification 结构体

```rust
pub struct SystemNotification {
    pub notification_type: SystemNotificationType,
    pub msg: String,
    pub data: Option<serde_json::Value>,
}
```

## 使用示例

```rust
use synthia_agent::types::{AgentEvent, AgentStatus, SystemNotification, SystemNotificationType};

fn handle_event(event: AgentEvent) {
    match event {
        AgentEvent::Status(status) => {
            println!("Status updated: {:?}", status);
        }
        AgentEvent::Message(msg) => {
            println!("New message: {:?}", msg);
        }
        AgentEvent::SystemNotification(notif) => {
            println!("System notification: {}", notif.message);
        }
        _ => {}
    }
}
```
