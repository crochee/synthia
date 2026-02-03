---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 人机协作 (Human-in-the-Loop)

## 1. 概述

Human-in-the-loop（人机交互）是 Synthia Agent 的重要特性，允许用户在 Agent 执行过程中进行干预、审批和反馈。本文档说明交互模式、中断机制、审批流程和反馈机制。

## 2. 交互模式

### 2.1 四种交互模式

```
┌─────────────────────────────────────────────────────────────┐
│                  Human-in-the-Loop Modes                     │
│                                                              │
│  1. Steering（软中断）                                       │
│     └── 用户在工具执行期间发消息                             │
│     └── 当前工具完成后，跳过剩余工具                         │
│     └── 已完成的工作保留                                     │
│                                                              │
│  2. Abort（硬中断）                                          │
│     └── 立即终止当前操作                                     │
│     └── 部分结果丢弃                                         │
│                                                              │
│  3. Approval（审批）                                         │
│     └── 工具调用前请求用户确认                               │
│     └── 用户批准或拒绝                                       │
│                                                              │
│  4. Feedback（反馈）                                         │
│     └── 用户对执行结果提供反馈                               │
│     └── Agent 根据反馈调整行为                               │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 模式对比

| 模式 | 触发时机 | 影响 | 适用场景 |
|------|----------|------|----------|
| Steering | 工具执行期间 | 跳过剩余工具 | 调整执行方向 |
| Abort | 任何时候 | 立即停止 | 紧急停止 |
| Approval | 工具调用前 | 等待确认 | 危险操作 |
| Feedback | 工具执行后 | 调整行为 | 改进结果 |

## 3. Steering（软中断）

### 3.1 工作原理

```
┌─────────────────────────────────────────────────────────────┐
│                      Steering Flow                           │
│                                                              │
│  Agent 执行工具 1                                            │
│     │                                                        │
│     ▼                                                        │
│  Agent 执行工具 2                                            │
│     │                                                        │
│     │  ◀── 用户发送新消息（Steering）                        │
│     │                                                        │
│     ▼                                                        │
│  Agent 完成工具 2                                            │
│     │                                                        │
│     ▼                                                        │
│  跳过工具 3、4...                                            │
│     │                                                        │
│     ▼                                                        │
│  Agent 处理用户新消息                                        │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 实现机制

```rust
pub struct SteeringMessage {
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

pub struct AgentExecutor {
    steering_rx: Receiver<SteeringMessage>,
    pending_tools: VecDeque<ToolCall>,
}

impl AgentExecutor {
    async fn execute_with_steering(&mut self) -> Result<()> {
        while let Some(tool_call) = self.pending_tools.pop_front() {
            // 检查是否有 steering 消息
            if let Ok(steering) = self.steering_rx.try_recv() {
                // 标记跳过的工具
                for pending in &self.pending_tools {
                    yield AgentEvent::Skipped(pending.clone());
                }
                
                // 处理 steering 消息
                yield AgentEvent::Steering(steering);
                return Ok(());
            }
            
            // 执行工具
            let result = self.execute_tool(&tool_call).await?;
            yield AgentEvent::ToolResult(result);
        }
        
        Ok(())
    }
}
```

### 3.3 使用示例

```rust
// 用户发送 steering 消息
let steering_tx = agent.get_steering_channel();

// 在另一个任务中发送
tokio::spawn(async move {
    tokio::time::sleep(Duration::from_secs(5)).await;
    steering_tx.send(SteeringMessage {
        content: "请先检查配置文件".to_string(),
        timestamp: Utc::now(),
    }).await.unwrap();
});

// Agent 会检测到 steering 消息并调整执行
let stream = agent.react(session_config, cancel_token).await;
```

## 4. Abort（硬中断）

### 4.1 工作原理

```
┌─────────────────────────────────────────────────────────────┐
│                       Abort Flow                             │
│                                                              │
│  Agent 执行工具 1                                            │
│     │                                                        │
│     │  ◀── 用户触发 Abort                                    │
│     │                                                        │
│     ▼                                                        │
│  立即终止工具 1                                               │
│     │                                                        │
│     ▼                                                        │
│  清理资源                                                     │
│     │                                                        │
│     ▼                                                        │
│  返回中断状态                                                 │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 4.2 实现机制

```rust
use tokio_util::sync::CancellationToken;

pub struct AgentExecutor {
    cancel_token: CancellationToken,
}

impl AgentExecutor {
    async fn execute_with_abort(&self) -> Result<()> {
        // 检查取消信号
        if self.cancel_token.is_cancelled() {
            return Err(AgentError::cancelled("Operation aborted"));
        }
        
        // 执行工具（支持取消）
        tokio::select! {
            result = self.execute_tool(&tool_call) => {
                yield AgentEvent::ToolResult(result?);
            }
            _ = self.cancel_token.cancelled() => {
                yield AgentEvent::Aborted;
                return Err(AgentError::cancelled("Operation aborted"));
            }
        }
        
        Ok(())
    }
}
```

### 4.3 使用示例

```rust
let cancel_token = CancellationToken::new();

// 在另一个任务中取消
tokio::spawn(async move {
    tokio::time::sleep(Duration::from_secs(10)).await;
    cancel_token.cancel();
});

// Agent 会检测到取消并立即停止
let stream = agent.react(session_config, cancel_token).await;
```

## 5. Approval（审批）

### 5.1 工作原理

```
┌─────────────────────────────────────────────────────────────┐
│                      Approval Flow                           │
│                                                              │
│  Agent 准备执行工具                                          │
│     │                                                        │
│     ▼                                                        │
│  检查工具是否需要审批                                        │
│     │                                                        │
│     ├── 不需要 → 直接执行                                    │
│     │                                                        │
│     └── 需要 → 发送审批请求                                  │
│         │                                                    │
│         ▼                                                    │
│     等待用户响应                                             │
│         │                                                    │
│         ├── 批准 → 执行工具                                  │
│         │                                                    │
│         └── 拒绝 → 跳过工具                                  │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 5.2 配置审批规则

```yaml
approval:
  enabled: true
  rules:
    - tool: "write"
      condition: "always"  # 总是需要审批
    
    - tool: "delete"
      condition: "always"  # 总是需要审批
    
    - tool: "exec"
      condition: "ask"     # 询问用户
      message: "执行命令需要您的确认"
    
    - tool: "edit"
      condition: "destructive"  # 仅破坏性操作需要审批
```

### 5.3 实现机制

```rust
pub struct ApprovalRequest {
    pub tool_name: String,
    pub tool_args: Value,
    pub reason: String,
    pub risk_level: RiskLevel,
}

pub enum ApprovalResponse {
    Approved,
    Rejected { reason: String },
    Modified { new_args: Value },
}

pub struct ApprovalManager {
    rules: Vec<ApprovalRule>,
    pending_approvals: HashMap<Uuid, Sender<ApprovalResponse>>,
}

impl ApprovalManager {
    pub async fn request_approval(
        &self,
        request: ApprovalRequest,
    ) -> Result<ApprovalResponse> {
        // 检查是否需要审批
        if !self.needs_approval(&request) {
            return Ok(ApprovalResponse::Approved);
        }
        
        // 发送审批请求
        let (tx, rx) = oneshot::channel();
        let id = Uuid::new_v4();
        self.pending_approvals.insert(id, tx);
        
        yield AgentEvent::ApprovalRequest {
            id,
            request: request.clone(),
        };
        
        // 等待响应
        let response = rx.await?;
        Ok(response)
    }
}
```

### 5.4 使用示例

```rust
// 配置审批规则
let approval_manager = ApprovalManager::new(vec![
    ApprovalRule {
        tool: "delete".to_string(),
        condition: ApprovalCondition::Always,
    },
]);

// Agent 执行时自动请求审批
let stream = agent.react(session_config, cancel_token).await;

// 处理审批请求
tokio::pin!(stream);
while let Some(event) = stream.next().await {
    match event {
        Ok(AgentEvent::ApprovalRequest { id, request }) => {
            // 显示审批请求给用户
            println!("工具 {} 需要审批: {}", request.tool_name, request.reason);
            
            // 用户批准或拒绝
            let response = if user_approves() {
                ApprovalResponse::Approved
            } else {
                ApprovalResponse::Rejected {
                    reason: "用户拒绝".to_string(),
                }
            };
            
            // 发送响应
            approval_manager.respond(id, response).await?;
        }
        _ => {}
    }
}
```

## 6. Feedback（反馈）

### 6.1 工作原理

```
┌─────────────────────────────────────────────────────────────┐
│                      Feedback Flow                           │
│                                                              │
│  Agent 执行工具                                              │
│     │                                                        │
│     ▼                                                        │
│  返回结果                                                     │
│     │                                                        │
│     ▼                                                        │
│  用户提供反馈                                                 │
│     │                                                        │
│     ├── 正面反馈 → 记录成功模式                              │
│     │                                                        │
│     ├── 负面反馈 → 记录失败模式                              │
│     │                                                        │
│     └── 纠正反馈 → Agent 调整行为                            │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 6.2 反馈类型

```rust
pub enum Feedback {
    Positive {
        comment: String,
    },
    Negative {
        comment: String,
        issue: String,
    },
    Corrective {
        expected: String,
        actual: String,
        correction: String,
    },
    Rating {
        score: u8,  // 1-5
        aspects: HashMap<String, u8>,
    },
}
```

### 6.3 反馈处理

```rust
pub struct FeedbackProcessor {
    feedback_history: Vec<FeedbackRecord>,
}

impl FeedbackProcessor {
    pub async fn process(&mut self, feedback: Feedback) -> Result<()> {
        // 记录反馈
        let record = FeedbackRecord {
            feedback,
            timestamp: Utc::now(),
            context: self.get_context(),
        };
        self.feedback_history.push(record);
        
        // 根据反馈调整行为
        match &record.feedback {
            Feedback::Corrective { correction, .. } => {
                // 应用纠正
                self.apply_correction(correction).await?;
            }
            Feedback::Negative { issue, .. } => {
                // 记录失败模式
                self.record_failure_pattern(issue).await?;
            }
            Feedback::Positive { .. } => {
                // 记录成功模式
                self.record_success_pattern().await?;
            }
            _ => {}
        }
        
        Ok(())
    }
}
```

### 6.4 使用示例

```rust
// Agent 执行完成
let result = agent.execute().await?;

// 用户提供反馈
let feedback = Feedback::Corrective {
    expected: "生成单元测试".to_string(),
    actual: "生成了集成测试".to_string(),
    correction: "请生成单元测试，而不是集成测试".to_string(),
};

// 发送反馈
agent.provide_feedback(feedback).await?;

// Agent 会根据反馈调整后续行为
```

## 7. 交互式会话

### 7.1 WebSocket 接口

```typescript
// 前端连接
const ws = new WebSocket('ws://localhost:8080/ws');

// 接收事件
ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  
  switch (data.type) {
    case 'approval_request':
      // 显示审批对话框
      showApprovalDialog(data);
      break;
    
    case 'steering':
      // 显示 steering 提示
      showSteeringPrompt(data);
      break;
    
    case 'tool_result':
      // 显示工具结果
      displayToolResult(data);
      break;
  }
};

// 发送审批响应
function respondApproval(id: string, approved: boolean) {
  ws.send(JSON.stringify({
    type: 'approval_response',
    id,
    approved,
  }));
}

// 发送 steering 消息
function sendSteering(message: string) {
  ws.send(JSON.stringify({
    type: 'steering',
    message,
  }));
}
```

### 7.2 REST API 接口

```bash
# 发送 steering 消息
curl -X POST http://localhost:8080/sessions/{session_id}/steering \
  -H "Content-Type: application/json" \
  -d '{"message": "请先检查配置文件"}'

# 响应审批请求
curl -X POST http://localhost:8080/approvals/{approval_id}/respond \
  -H "Content-Type: application/json" \
  -d '{"approved": true}'

# 提供反馈
curl -X POST http://localhost:8080/sessions/{session_id}/feedback \
  -H "Content-Type: application/json" \
  -d '{
    "type": "corrective",
    "expected": "生成单元测试",
    "actual": "生成了集成测试",
    "correction": "请生成单元测试"
  }'
```

## 8. 最佳实践

### 8.1 交互设计原则

1. **最小干预**：只在必要时请求用户干预
2. **清晰提示**：提供清晰的提示和选项
3. **快速响应**：快速响应用户输入
4. **可撤销**：允许用户撤销操作

### 8.2 审批设计原则

1. **合理粒度**：审批粒度适中，不过于频繁
2. **上下文信息**：提供足够的上下文信息
3. **默认安全**：默认选择安全的选项
4. **批量审批**：支持批量审批相似操作

### 8.3 反馈设计原则

1. **及时反馈**：在适当时机请求反馈
2. **简单易用**：反馈机制简单易用
3. **可操作**：反馈应该可操作
4. **持续改进**：根据反馈持续改进

## 9. 相关文档

- [Agent执行流程](../core-concepts/agent-execution.md)
- [错误恢复](error-recovery.md)
- [安全最佳实践](security-best-practices.md)

## 10. 参考资料

- [Pi-Mono Steering Messages](https://github.com/pi-mono/agent)
- [Agent-Zero Intervention](https://github.com/frdel/agent-zero)
- [LangChain Human-in-the-Loop](https://python.langchain.com/docs/modules/agents/)
