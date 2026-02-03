# Event Handler 模块

事件处理模块，提供 Agent 事件的统一处理接口。

## AgentEventHandler Trait

```rust
#[async_trait]
pub trait AgentEventHandler: Send + Sync {
    async fn on_event(&self, agent_name: &str, event: &AgentEvent);
}
```

## 实现方式

### 函数指针实现

允许使用简单函数作为事件处理器：

```rust
#[async_trait]
impl<F> AgentEventHandler for F
where
    F: Fn(&str, &AgentEvent) + Send + Sync,
{
    async fn on_event(&self, agent_name: &str, event: &AgentEvent) {
        self(agent_name, event)
    }
}
```

### Arc 包装实现

允许跨多个 Agent 共享事件处理器：

```rust
#[async_trait]
impl AgentEventHandler for Arc<dyn Fn(&str, &AgentEvent) + Send + Sync>
```

## 使用示例

```rust
use synthia_agent::event_handler::AgentEventHandler;
use synthia_agent::types::AgentEvent;

// 闭包方式
let handler = |agent_name: &str, event: &AgentEvent| {
    println!("Agent {}: {:?}", agent_name, event);
};

// Trait 实现
#[async_trait]
impl AgentEventHandler for MyHandler {
    async fn on_event(&self, agent_name: &str, event: &AgentEvent) {
        // 处理事件
    }
}
```

## 事件类型

事件类型定义在 `types` 模块中：

```rust
use synthia_agent::types::AgentEvent;
```

可用事件变体：

- `AgentEvent::Message` - 消息事件
- `AgentEvent::McpNotification` - MCP 通知
- `AgentEvent::ModelChange` - 模型变更
- `AgentEvent::HistoryReplaced` - 历史替换
- `AgentEvent::SystemNotification` - 系统通知
- `AgentEvent::Status` - 状态变更
