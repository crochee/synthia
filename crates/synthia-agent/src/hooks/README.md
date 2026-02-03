# Hooks 模块

生命周期钩子模块，提供事件驱动的扩展机制，允许在 Agent 生命周期中注入自定义逻辑。

## 核心组件

| 组件 | 功能描述 |
|------|----------|
| `Hook` | 钩子 trait，支持自定义扩展 |
| `HookRegistry` | 钩子注册表，管理钩子的注册、注销和事件触发 |
| `HookEvent` | 事件类型 |
| `LoggingHook` | 内置日志钩子，记录事件到 tracing |

## 交互顺序

```
Agent 生命周期 → 事件触发 → 钩子并发执行 → 继续流程
```

## 事件类型

```rust
pub enum HookEvent {
    BeforeAgentStart { session_id: String },
    AfterAgentEnd { session_id: String, success: bool },
    BeforeLLMCall { model: String, message_count: usize },
    AfterLLMCall { model: String, tokens_used: Option<u64>, success: bool },
    BeforeToolCall { tool: String, args: Value },
    AfterToolCall { tool: String, args: Value, success: bool },
    SessionStart { session_id: String },
    SessionEnd { session_id: String, message_count: usize },
    ContextCompaction { messages_removed: usize, tokens_saved: u64 },
}
```

## 使用示例

```rust
use synthia_agent::hooks::{Hook, HookEvent, HookRegistry, HookPtr};
use async_trait::async_trait;

struct MyHook;

#[async_trait]
impl Hook for MyHook {
    fn name(&self) -> &str { "my_hook" }

    async fn on_event(&self, event: &HookEvent) -> Result<()> {
        println!("Event: {:?}", event);
        Ok(())
    }
}

let registry = HookRegistry::new();
registry.register(Arc::new(MyHook)).await;
registry.emit(&HookEvent::SessionStart { session_id: "test".into() }).await;
```

## HookRegistry 方法

- `new()` - 创建空注册表
- `register(hook)` - 注册钩子
- `unregister(name)` - 按名称注销钩子，返回是否成功
- `emit(event)` - 并发触发所有钩子
- `hook_count()` - 获取已注册钩子数量

## 钩子执行

事件触发时，所有钩子通过 `join_all` 并发执行，错误会被记录但不会阻止其他钩子执行。
