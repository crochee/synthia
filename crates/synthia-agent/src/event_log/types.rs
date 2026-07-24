use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 事件日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLogEntry {
    /// 事件时间戳
    pub timestamp: DateTime<Utc>,
    /// 会话 ID
    pub session_id: String,
    /// 事件类型
    pub event_type: String,
    /// 事件数据（脱敏后）
    pub data: serde_json::Value,
}

/// 事件类型枚举
#[derive(Debug, Clone)]
pub enum EventType {
    ToolCall {
        name: String,
        args: serde_json::Value,
    },
    ToolResult {
        name: String,
        output: String,
        is_error: bool,
    },
    ModelRequest {
        model: String,
        prompt_tokens: usize,
    },
    ModelResponse {
        model: String,
        completion_tokens: usize,
    },
    Error {
        source: String,
        message: String,
    },
    Decision {
        description: String,
    },
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::ToolCall { .. } => "tool_call",
            EventType::ToolResult { .. } => "tool_result",
            EventType::ModelRequest { .. } => "model_request",
            EventType::ModelResponse { .. } => "model_response",
            EventType::Error { .. } => "error",
            EventType::Decision { .. } => "decision",
        }
    }
}
