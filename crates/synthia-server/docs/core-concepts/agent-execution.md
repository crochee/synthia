---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# Agent 执行流程

## 1. 概述

Synthia Agent 采用 **ReAct (Reasoning and Acting)** 模式实现智能体的推理和执行循环。本文档详细说明 Agent 的执行流程、状态管理、工具选择机制和生命周期。

## 2. 核心架构

### 2.1 Agent 组件

Agent 由以下核心组件构成：

```
Agent
├── config: AgentConfig              # Agent 配置
├── tool_registry: ToolRegistry      # 工具注册表
├── context_manager: ContextManager  # 上下文管理
├── session_manager: SessionManager  # 会话管理
├── model_router: ModelRouter        # 模型路由
├── hook_registry: HookRegistry      # 生命周期钩子
├── skill_tool: SkillTool            # 技能工具
├── guardian: Guardian               # 安全审查
├── control: AgentControl            # 生命周期控制
├── prompt_state: PromptState        # 提示状态
└── loop_detector: LoopDetector      # 循环检测器
```

### 2.2 执行流程概览

```
用户输入 → 构建提示 → 调用LLM → 工具执行 → 结果返回 → 循环或结束
```

## 3. ReAct 推理循环

### 3.1 循环流程

ReAct 循环是 Agent 的核心执行机制，遵循以下步骤：

```
┌─────────────────────────────────────────────────────────────┐
│                      ReAct Loop                              │
│                                                              │
│  1. 接收用户消息                                             │
│     │                                                        │
│     ▼                                                        │
│  2. 检查退出条件                                             │
│     ├── 是否取消？                                           │
│     ├── 是否达到最大步数？                                   │
│     └── 是否检测到循环？                                     │
│     │                                                        │
│     ▼                                                        │
│  3. 获取并压缩对话历史                                       │
│     │                                                        │
│     ▼                                                        │
│  4. 调用 LLM 获取响应                                        │
│     │                                                        │
│     ▼                                                        │
│  5. 检查 stop_reason                                         │
│     ├── "stop" → 返回文本响应（结束）                        │
│     ├── "tool_use" → 执行工具                                │
│     └── 其他 → 错误处理                                      │
│     │                                                        │
│     ▼                                                        │
│  6. 执行工具调用                                             │
│     │                                                        │
│     ▼                                                        │
│  7. 将工具结果追加到对话                                     │
│     │                                                        │
│     └──────────────────────────────────────┐                │
│                                              │                │
│                                              ▼                │
│                                        返回步骤 2             │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 代码实现

ReAct 循环的核心实现：

```rust
pub async fn react(
    &self,
    session_config: SessionConfig,
    cancel_token: CancellationToken,
) -> BoxStream<'static, AgentEvent> {
    let mut state = ReactState::new(session_config, cancel_token);

    loop {
        state.increment_step();

        // 检查退出条件
        if let Some(status) = agent.check_exit_conditions(&state).await {
            agent.control.update_status(status.clone());
            yield AgentEvent::Status(status);
            return;
        }

        // 处理当前步骤
        let result = agent.process_react_step(&state, &tools).await;
        // ... 处理结果
    }
}
```

## 4. 状态管理

### 4.1 AgentStatus 状态

Agent 在执行过程中会经历以下状态：

| 状态 | 说明 | 触发条件 |
|------|------|----------|
| `PendingInit` | 等待初始化 | Agent 创建时 |
| `Running` | 运行中 | 开始执行 ReAct 循环 |
| `MaxStepsReached(u32)` | 达到最大步数 | current_step >= max_steps |
| `Completed` | 完成 | LLM 返回 stop_reason="stop" |
| `Errored(String)` | 错误 | 执行过程中发生错误 |
| `Shutdown` | 已关闭 | Agent 被关闭 |
| `Cancelled` | 已取消 | CancellationToken 被触发 |
| `LoopDetected(String)` | 检测到循环 | 循环检测器发现重复模式 |

### 4.2 状态转换图

```
┌─────────────┐
│ PendingInit │
└──────┬──────┘
       │
       ▼
┌─────────────┐
│   Running   │◀────────────┐
└──────┬──────┘             │
       │                    │
       ├────────────────────┤
       │                    │
       ▼                    ▼
┌─────────────┐      ┌─────────────┐
│  Completed  │      │ MaxSteps    │
└─────────────┘      │ Reached     │
                     └─────────────┘
       │
       ▼
┌─────────────┐      ┌─────────────┐
│  Cancelled  │      │   Errored   │
└─────────────┘      └─────────────┘
       │
       ▼
┌─────────────┐      ┌─────────────┐
│   Shutdown  │      │ LoopDetected│
└─────────────┘      └─────────────┘
```

### 4.3 AgentControl

`AgentControl` 提供 Agent 生命周期管理和状态监控：

```rust
use synthia_agent::agent::AgentControl;
use synthia_agent::types::AgentStatus;

let control = AgentControl::new();

// 更新状态
control.update_status(AgentStatus::Running);

// 订阅状态变化
let mut receiver = control.subscribe_status();

// 检查是否为最终状态
if control.is_final_status() {
    println!("Agent has finished");
}
```

**核心方法**：

| 方法 | 说明 |
|------|------|
| `update_status(status)` | 更新 Agent 状态 |
| `subscribe_status()` | 订阅状态变化（返回 Receiver） |
| `get_status()` | 获取当前状态 |
| `is_final_status()` | 检查是否为最终状态 |

**最终状态**：
- `Completed`
- `Errored`
- `Shutdown`
- `Cancelled`

## 5. 工具选择机制

### 5.1 工具调用流程

当 LLM 返回 `stop_reason="tool_use"` 时，Agent 会执行工具调用：

```
┌─────────────────────────────────────────────────────────────┐
│                      Tool Execution                          │
│                                                              │
│  1. 提取工具调用信息                                         │
│     ├── tool_name: 工具名称                                  │
│     └── tool_input: 工具参数                                 │
│     │                                                        │
│     ▼                                                        │
│  2. 查找工具                                                 │
│     ├── 在 tool_registry 中查找                              │
│     └── 检查工具是否可用                                     │
│     │                                                        │
│     ▼                                                        │
│  3. 执行工具                                                 │
│     ├── 并发执行（最多 max_concurrent_tools 个）             │
│     ├── 记录执行结果                                         │
│     └── 触发钩子事件                                         │
│     │                                                        │
│     ▼                                                        │
│  4. 处理结果                                                 │
│     ├── 成功：将结果追加到对话                               │
│     └── 失败：记录错误并继续                                 │
│     │                                                        │
│     ▼                                                        │
│  5. 记录到循环检测器                                         │
│     ├── 记录工具名称                                         │
│     ├── 记录参数哈希                                         │
│     └── 记录执行结果                                         │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 5.2 并发执行

Agent 支持并发执行多个工具调用：

```rust
let tool_futures = tool_uses.into_iter().map(|tool_use| {
    async move { agent.execute_single_tool(tool_use).await }
});

let mut concurrent_stream = futures::stream::iter(tool_futures)
    .buffer_unordered(max_concurrent);
```

**配置参数**：

| 参数 | 说明 | 默认值 |
|------|------|--------|
| `max_concurrent_tools` | 最大并发工具数 | 5 |

### 5.3 工具过滤

Agent 会根据配置过滤可用工具：

```rust
let tools = agent.get_filtered_tools().await;
```

**过滤规则**：

| 配置 | 说明 |
|------|------|
| `allowed_tools` | 允许使用的工具列表（白名单） |
| `denied_tools` | 禁止使用的工具列表（黑名单） |

## 6. 退出条件

### 6.1 取消执行

通过 `CancellationToken` 可以随时取消 Agent 执行：

```rust
use tokio_util::sync::CancellationToken;

let cancel_token = CancellationToken::new();

// 在另一个任务中取消
tokio::spawn(async move {
    tokio::time::sleep(Duration::from_secs(10)).await;
    cancel_token.cancel();
});

// Agent 会检测到取消并退出
let stream = agent.react(session_config, cancel_token).await;
```

### 6.2 最大步数限制

Agent 会检查是否达到最大步数：

```rust
if state.current_step >= self.session_config.max_steps {
    return Some(AgentStatus::MaxStepsReached(state.current_step));
}
```

**配置**：

```yaml
agents:
  code-reviewer:
    max_steps: 50  # 最大执行步数
```

### 6.3 循环检测

Agent 内置循环检测机制，防止无限循环。详细的检测实现和配置请参考 [错误恢复](../guides/error-recovery.md#4-循环检测)。

```rust
if let Some(detection) = state.loop_detector.detect_loop() {
    tracing::warn!(
        tool_name = %detection.tool_name,
        occurrences = detection.occurrences,
        "Loop detected"
    );
    return Some(AgentStatus::LoopDetected(detection.tool_name));
}
```

**检测机制**：

| 检测类型 | 说明 |
|----------|------|
| Generic Repeat | 同工具同参数重复 N 次 |
| Poll No Progress | 轮询结果无变化 |
| Ping-Pong | 两工具交替循环 |
| Circuit Breaker | 全局连续 30 次无进展 |

更多详细信息请查看 [错误恢复 - 循环检测](../guides/error-recovery.md#4-循环检测)。

### 6.4 自然结束

当 LLM 返回 `stop_reason="stop"` 时，Agent 自然结束：

```rust
match create_result.stop_reason.as_deref() {
    Some("stop") => {
        yield Ok(AgentEvent::Status(AgentStatus::Completed));
        return;
    }
    // ...
}
```

## 7. 生命周期钩子

### 7.1 HookEvent 事件

Agent 在执行过程中会触发以下钩子事件：

| 事件 | 说明 | 触发时机 |
|------|------|----------|
| `SessionStart` | 会话开始 | ReAct 循环开始时 |
| `SessionEnd` | 会话结束 | ReAct 循环结束时 |
| `BeforeToolCall` | 工具调用前 | 执行工具前 |
| `AfterToolCall` | 工具调用后 | 执行工具后 |
| `BeforeModelCall` | 模型调用前 | 调用 LLM 前 |
| `AfterModelCall` | 模型调用后 | 调用 LLM 后 |

### 7.2 使用钩子

```rust
use synthia_agent::hooks::{HookRegistry, HookEvent};

let hook_registry = HookRegistry::new();

hook_registry.register(|event: &HookEvent| {
    match event {
        HookEvent::BeforeToolCall { tool, args } => {
            println!("即将执行工具: {}", tool);
        }
        HookEvent::AfterToolCall { tool, success, .. } => {
            println!("工具执行完成: {} (成功: {})", tool, success);
        }
        _ => {}
    }
    async { Ok(()) }
});
```

## 8. AgentEvent 事件流

### 8.1 事件类型

Agent 执行过程中会产生以下事件：

| 事件 | 说明 |
|------|------|
| `Message(SamplingMessage)` | 消息事件（用户或助手） |
| `Status(AgentStatus)` | 状态事件 |
| `SystemNotification(String)` | 系统通知 |

### 8.2 消费事件流

```rust
use futures::StreamExt;

let stream = agent.react(session_config, cancel_token).await;

tokio::pin!(stream);

while let Some(event_result) = stream.next().await {
    match event_result {
        Ok(AgentEvent::Message(msg)) => {
            println!("收到消息: {:?}", msg);
        }
        Ok(AgentEvent::Status(status)) => {
            println!("状态变化: {:?}", status);
            if status.is_final() {
                break;
            }
        }
        Ok(AgentEvent::SystemNotification(notification)) => {
            println!("系统通知: {}", notification);
        }
        Err(e) => {
            eprintln!("错误: {}", e);
            break;
        }
    }
}
```

## 9. 异步执行模型

### 9.1 流式响应

Agent 使用异步流（Stream）返回结果，支持实时处理：

```rust
pub async fn react(
    &self,
    session_config: SessionConfig,
    cancel_token: CancellationToken,
) -> BoxStream<'static, AgentEvent> {
    // 返回异步流
    Box::pin(async_stream::stream! {
        // 产生事件
        yield AgentEvent::Message(msg);
        yield AgentEvent::Status(status);
    })
}
```

### 9.2 并发处理

Agent 内部使用 Tokio 异步运行时，支持：

- **并发工具执行**：多个工具同时执行
- **异步 I/O**：非阻塞文件、网络操作
- **取消传播**：CancellationToken 跨任务传播

## 10. 最佳实践

### 10.1 设置合理的最大步数

```yaml
agents:
  simple-task:
    max_steps: 10  # 简单任务
  
  complex-task:
    max_steps: 100  # 复杂任务
```

### 10.2 使用钩子监控执行

```rust
hook_registry.register(|event: &HookEvent| {
    match event {
        HookEvent::BeforeToolCall { tool, args } => {
            // 记录工具调用
            log_tool_call(tool, args);
        }
        HookEvent::AfterToolCall { tool, success, .. } => {
            // 记录执行结果
            log_tool_result(tool, success);
        }
        _ => {}
    }
    async { Ok(()) }
});
```

### 10.3 处理取消和超时

```rust
use tokio::time::{timeout, Duration};

let cancel_token = CancellationToken::new();

// 设置超时
let result = timeout(
    Duration::from_secs(60),
    async {
        let stream = agent.react(session_config, cancel_token.clone()).await;
        let events: Vec<_> = stream.collect().await;
        events
    }
).await;

match result {
    Ok(events) => println!("完成"),
    Err(_) => {
        cancel_token.cancel();
        println!("超时，已取消");
    }
}
```

### 10.4 监控状态变化

```rust
let control = agent.control.clone();
let mut status_receiver = control.subscribe_status();

tokio::spawn(async move {
    while status_receiver.changed().await.is_ok() {
        let status = status_receiver.borrow().clone();
        println!("状态更新: {:?}", status);
        
        if control.is_final_status() {
            break;
        }
    }
});
```

## 11. 相关文档

- [记忆系统](memory-system.md)
- [上下文管理](context-management.md)
- [工具系统](tool-system.md)
- [错误恢复](../guides/error-recovery.md)

## 12. 参考资料

- [ReAct: Synergizing Reasoning and Acting in Language Models](https://arxiv.org/abs/2210.03629)
- [OpenAI Assistants API](https://platform.openai.com/docs/assistants)
- [Anthropic Claude Agent SDK](https://www.anthropic.com/engineering/building-agents-with-the-claude-agent-sdk)
