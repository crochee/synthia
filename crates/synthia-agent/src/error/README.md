# Error 模块

错误类型定义模块，提供 synthia-agent 所有错误类型的统一定义和处理机制。

## AgentError 枚举

```rust
#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Provider error: {0}")]
    Provider(#[from] ProviderError),

    #[error("Tool '{tool}' failed: {message}")]
    ToolError { tool: String, message: String },

    #[error("Tool approval required: {0}")]
    ToolApprovalRequired(String),

    #[error("Session error: {0}")]
    SessionError(String),

    #[error("Context error: {0}")]
    ContextError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("MCP server error: {0}")]
    McpServerError(#[from] rmcp::ServiceError),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Operation timeout: {0}")]
    Timeout(String),

    #[error("Rate limited, retry after {retry_after:?} seconds")]
    RateLimited { retry_after: Option<u64> },

    #[error("Context window exceeded: current {current} tokens, limit {limit}")]
    ContextWindowExceeded { current: usize, limit: usize },

    #[error("File conflict: {path} has been modified")]
    FileConflict { path: String },

    #[error("Internal error: {0}")]
    InternalError(String),
}
```

## 错误创建方法

```rust
use synthia_agent::AgentError;

// 工具错误
AgentError::tool("read_file", "file not found");
AgentError::tool_error("something went wrong");

// 会话错误
AgentError::session("session not found");

// 上下文错误
AgentError::context("context window exceeded");

// 配置错误
AgentError::config("missing required field");

// 验证错误
AgentError::validation("invalid input");

// 超时错误
AgentError::timeout("operation took too long");

// 内部错误
AgentError::internal("unexpected state");

// 数据库错误
AgentError::database("connection failed");

// 文件冲突
AgentError::file_conflict("/path/to/file");

// 速率限制
AgentError::rate_limited(Some(60));

// 上下文窗口超出
AgentError::context_window_exceeded(10000, 8000);
```

## 错误检查方法

```rust
impl AgentError {
    pub fn is_timeout(&self) -> bool;
    pub fn is_rate_limited(&self) -> bool;
    pub fn is_context_window_exceeded(&self) -> bool;
}
```

## From 实现

AgentError 实现了以下类型的转换：

```rust
impl From<sqlx::Error> for AgentError
impl From<regex::Error> for AgentError
impl From<tokio::task::JoinError> for AgentError
impl From<String> for AgentError
impl From<&str> for AgentError
```
