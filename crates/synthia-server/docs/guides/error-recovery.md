---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 错误恢复指南

## 1. 概述

Synthia Agent 具有完善的错误恢复机制，能够自动处理各种错误情况并恢复执行。本文档说明错误类型、恢复策略、循环检测和错误处理最佳实践。

## 2. 错误类型

### 2.1 错误分类

| 类型 | 说明 | 示例 |
|------|------|------|
| **LLM错误** | 模型调用失败 | API超时、上下文超限、模型不可用 |
| **工具错误** | 工具执行失败 | 文件不存在、权限不足、参数错误 |
| **MCP错误** | MCP服务器错误 | 服务器启动失败、通信错误 |
| **系统错误** | 系统级错误 | 内存不足、磁盘满、网络中断 |

### 2.2 错误定义

```rust
pub enum AgentError {
    InvalidInput(String),
    ContextTooLong(String),
    ModelError(String),
    ToolError(String),
    McpError(String),
    Timeout(String),
    NotFound(String),
    PermissionDenied(String),
    Internal(String),
}
```

## 3. 恢复策略

### 3.1 自动重试

```rust
pub struct RetryConfig {
    pub max_retries: usize,           // 最大重试次数
    pub base_delay: Duration,         // 基础延迟
    pub max_delay: Duration,          // 最大延迟
    pub multiplier: f64,              // 延迟倍数
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            multiplier: 2.0,
        }
    }
}
```

### 3.2 指数退避

```rust
async fn execute_with_retry<T, E, F, Fut>(
    f: F,
    config: &RetryConfig,
) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut delay = config.base_delay;
    
    for attempt in 0..=config.max_retries {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) if attempt < config.max_retries => {
                tokio::time::sleep(delay).await;
                delay = std::cmp::min(
                    Duration::from_secs_f64(delay.as_secs_f64() * config.multiplier),
                    config.max_delay,
                );
            }
            Err(e) => return Err(e),
        }
    }
    
    unreachable!()
}
```

### 3.3 降级处理

```rust
async fn execute_tool_with_fallback(
    tool_name: &str,
    args: &Value,
) -> Result<ToolResult> {
    // 尝试主工具
    match execute_tool(tool_name, args).await {
        Ok(result) => Ok(result),
        Err(e) => {
            tracing::warn!("Tool {} failed: {}, trying fallback", tool_name, e);
            
            // 尝试降级方案
            match get_fallback_tool(tool_name) {
                Some(fallback) => execute_tool(fallback, args).await,
                None => Err(e),
            }
        }
    }
}
```

### 3.4 失败转移

```rust
pub struct FailoverConfig {
    pub primary: String,
    pub fallbacks: Vec<String>,
}

async fn execute_with_failover(
    config: &FailoverConfig,
    args: &Value,
) -> Result<ToolResult> {
    let mut providers = vec![&config.primary];
    providers.extend(config.fallbacks.iter());
    
    for provider in providers {
        match execute_tool(provider, args).await {
            Ok(result) => return Ok(result),
            Err(e) => {
                tracing::warn!("Provider {} failed: {}", provider, e);
                continue;
            }
        }
    }
    
    Err(AgentError::internal("All providers failed"))
}
```

## 4. 循环检测

### 4.1 检测机制

Agent 内置四层循环检测机制：

```
┌─────────────────────────────────────────────────────────────┐
│                    Loop Detection                            │
│                                                              │
│  Layer 1: Generic Repeat                                     │
│  └── 同工具同参数重复 N 次                                   │
│                                                              │
│  Layer 2: Poll No Progress                                   │
│  └── 轮询结果无变化                                          │
│                                                              │
│  Layer 3: Ping-Pong                                          │
│  └── 两工具交替循环                                          │
│                                                              │
│  Layer 4: Circuit Breaker                                    │
│  └── 全局连续 30 次无进展                                    │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 4.2 检测实现

```rust
pub struct LoopDetector {
    history: VecDeque<OperationPattern>,
    max_history: usize,
    detection_threshold: usize,
}

#[derive(Clone, Debug)]
pub struct OperationPattern {
    pub tool_name: String,
    pub args_hash: u64,
    pub timestamp: DateTime<Utc>,
    pub outcome: Outcome,
}

#[derive(Clone, Copy, Debug)]
pub enum Outcome {
    Success,
    Failure,
}

impl LoopDetector {
    pub fn record(&mut self, pattern: OperationPattern) {
        self.history.push_back(pattern);
        if self.history.len() > self.max_history {
            self.history.pop_front();
        }
    }
    
    pub fn detect_loop(&self) -> Option<LoopDetection> {
        // Layer 1: Generic Repeat
        if let Some(detection) = self.detect_generic_repeat() {
            return Some(detection);
        }
        
        // Layer 2: Poll No Progress
        if let Some(detection) = self.detect_poll_no_progress() {
            return Some(detection);
        }
        
        // Layer 3: Ping-Pong
        if let Some(detection) = self.detect_ping_pong() {
            return Some(detection);
        }
        
        // Layer 4: Circuit Breaker
        if let Some(detection) = self.detect_circuit_breaker() {
            return Some(detection);
        }
        
        None
    }
}
```

### 4.3 检测结果

```rust
pub struct LoopDetection {
    pub tool_name: String,
    pub occurrences: usize,
    pub pattern: LoopPattern,
    pub suggestion: String,
}

pub enum LoopPattern {
    GenericRepeat,      // 重复调用
    PollNoProgress,     // 轮询无进展
    PingPong,           // 交替循环
    CircuitBreaker,     // 全局无进展
}
```

## 5. 错误处理流程

### 5.1 处理流程

```
┌─────────────────────────────────────────────────────────────┐
│                    Error Handling Flow                       │
│                                                              │
│  1. 捕获错误                                                 │
│     │                                                        │
│     ▼                                                        │
│  2. 分类错误                                                 │
│     ├── 可恢复错误                                           │
│     └── 不可恢复错误                                         │
│     │                                                        │
│     ▼                                                        │
│  3. 选择恢复策略                                             │
│     ├── 重试                                                 │
│     ├── 降级                                                 │
│     ├── 失败转移                                             │
│     └── 放弃                                                 │
│     │                                                        │
│     ▼                                                        │
│  4. 执行恢复                                                 │
│     │                                                        │
│     ▼                                                        │
│  5. 记录日志                                                 │
│     │                                                        │
│     ▼                                                        │
│  6. 返回结果                                                 │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 5.2 错误处理示例

```rust
async fn handle_tool_error(
    error: AgentError,
    tool_name: &str,
    args: &Value,
    context: &mut ToolContext,
) -> Result<ToolResult> {
    // 1. 记录错误
    tracing::error!(
        tool_name = %tool_name,
        error = %error,
        "Tool execution failed"
    );
    
    // 2. 检查是否可重试
    if is_retryable(&error) {
        // 3. 检查重试次数
        if context.retry_count < MAX_RETRIES {
            context.retry_count += 1;
            
            // 4. 指数退避
            let delay = Duration::from_millis(100 * 2_u64.pow(context.retry_count));
            tokio::time::sleep(delay).await;
            
            // 5. 重试
            return execute_tool(tool_name, args, context).await;
        }
    }
    
    // 6. 尝试降级
    if let Some(fallback) = get_fallback_tool(tool_name) {
        tracing::info!(
            tool_name = %tool_name,
            fallback = %fallback,
            "Trying fallback tool"
        );
        return execute_tool(&fallback, args, context).await;
    }
    
    // 7. 返回错误
    Err(error)
}
```

## 6. 错误日志

### 6.1 日志记录

```rust
pub struct ErrorLog {
    pub timestamp: DateTime<Utc>,
    pub error_type: String,
    pub error_message: String,
    pub context: ErrorContext,
    pub recovery_attempted: bool,
    pub recovery_successful: Option<bool>,
}

pub struct ErrorContext {
    pub session_id: String,
    pub tool_name: Option<String>,
    pub step: u32,
    pub conversation_length: usize,
}
```

### 6.2 日志分析

```rust
pub struct ErrorAnalyzer {
    errors: Vec<ErrorLog>,
}

impl ErrorAnalyzer {
    pub fn analyze(&self) -> ErrorAnalysis {
        ErrorAnalysis {
            total_errors: self.errors.len(),
            by_type: self.group_by_type(),
            by_tool: self.group_by_tool(),
            recovery_rate: self.calculate_recovery_rate(),
            common_patterns: self.find_common_patterns(),
        }
    }
    
    fn calculate_recovery_rate(&self) -> f64 {
        let recovered = self.errors
            .iter()
            .filter(|e| e.recovery_successful == Some(true))
            .count();
        
        recovered as f64 / self.errors.len() as f64
    }
}
```

## 7. 五层错误恢复

### 7.1 恢复层级

```
┌─────────────────────────────────────────────────────────────┐
│                  Five-Layer Error Recovery                   │
│                                                              │
│  Layer 1: Truncate                                           │
│  └── 截断大输出，重试                                        │
│                                                              │
│  Layer 2: Retry                                              │
│  └── 指数退避重试                                            │
│                                                              │
│  Layer 3: Fallback                                           │
│  └── 使用降级方案                                            │
│                                                              │
│  Layer 4: Auto-compact                                       │
│  └── 自动压缩上下文                                          │
│                                                              │
│  Layer 5: Reset                                              │
│  └── 重置会话                                                │
│                                                              │
│  Layer 6: Fail-fast                                          │
│  └── 快速失败，返回错误                                      │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 7.2 实现示例

```rust
async fn recover_from_error(
    error: AgentError,
    context: &mut ExecutionContext,
) -> Result<RecoveryResult> {
    // Layer 1: Truncate
    if is_output_too_large(&error) {
        context.truncate_output();
        return Ok(RecoveryResult::Retried);
    }
    
    // Layer 2: Retry
    if is_retryable(&error) && context.retry_count < MAX_RETRIES {
        context.retry_count += 1;
        let delay = calculate_backoff(context.retry_count);
        tokio::time::sleep(delay).await;
        return Ok(RecoveryResult::Retried);
    }
    
    // Layer 3: Fallback
    if let Some(fallback) = get_fallback(&error) {
        return Ok(RecoveryResult::Fallback(fallback));
    }
    
    // Layer 4: Auto-compact
    if is_context_error(&error) {
        context.compact().await?;
        return Ok(RecoveryResult::Compacted);
    }
    
    // Layer 5: Reset
    if context.can_reset() {
        context.reset().await?;
        return Ok(RecoveryResult::Reset);
    }
    
    // Layer 6: Fail-fast
    Err(error)
}
```

## 8. 最佳实践

### 8.1 错误处理原则

1. **快速失败**：对于不可恢复的错误，快速失败
2. **优雅降级**：对于可恢复的错误，尝试降级
3. **透明报告**：向用户清楚地报告错误
4. **自动恢复**：尽可能自动恢复，减少用户干预

### 8.2 日志记录原则

1. **记录上下文**：记录足够的上下文信息
2. **分级日志**：使用不同级别的日志
3. **结构化日志**：使用结构化格式便于分析
4. **敏感信息**：避免记录敏感信息

### 8.3 重试策略原则

1. **限制重试次数**：避免无限重试
2. **指数退避**：使用指数退避避免雪崩
3. **抖动**：添加随机抖动避免同步重试
4. **断路器**：使用断路器防止级联失败

## 9. 相关文档

- [Agent执行流程](../core-concepts/agent-execution.md)
- [上下文管理](../core-concepts/context-management.md)
- [错误码表](../api-reference/ERROR_CODES.md)

## 10. 参考资料

- [OpenClaw Loop Detection](https://github.com/openclaw/agent)
- [LangChain Error Handling](https://python.langchain.com/docs/modules/agents/)
- [Agent-Zero Intervention](https://github.com/frdel/agent-zero)
