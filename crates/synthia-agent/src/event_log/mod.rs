pub mod types;

use std::{path::PathBuf, sync::Arc};

use chrono::Utc;
use tokio::sync::Mutex;
use types::{EventLogEntry, EventType};

/// 默认批量大小
const DEFAULT_BATCH_SIZE: usize = 10;

/// 工具输出最大长度（字节）
const MAX_OUTPUT_BYTES: usize = 10 * 1024;

/// 事件日志写入器
///
/// 支持异步批量写入和 fsync 持久化
pub struct EventLogger {
    /// 日志目录
    log_dir: PathBuf,
    /// 批量缓冲区
    buffer: Arc<Mutex<Vec<EventLogEntry>>>,
    /// 批量大小阈值
    batch_size: usize,
}

impl EventLogger {
    pub fn new(log_dir: PathBuf, batch_size: usize) -> Self {
        Self {
            log_dir,
            buffer: Arc::new(Mutex::new(Vec::with_capacity(batch_size))),
            batch_size,
        }
    }

    /// 记录事件
    pub async fn log(
        &self,
        session_id: &str,
        event_type: EventType,
    ) -> anyhow::Result<()> {
        let data = self.sanitize_event_data(&event_type);

        let entry = EventLogEntry {
            timestamp: Utc::now(),
            session_id: session_id.to_string(),
            event_type: event_type.as_str().to_string(),
            data,
        };

        let mut buffer = self.buffer.lock().await;
        buffer.push(entry);

        if buffer.len() >= self.batch_size {
            self.flush_buffer(&mut buffer).await?;
        }

        Ok(())
    }

    /// 刷新缓冲区
    pub async fn flush(&self) -> anyhow::Result<()> {
        let mut buffer = self.buffer.lock().await;
        self.flush_buffer(&mut buffer).await
    }

    /// 内部刷新逻辑
    async fn flush_buffer(
        &self,
        buffer: &mut Vec<EventLogEntry>,
    ) -> anyhow::Result<()> {
        if buffer.is_empty() {
            return Ok(());
        }

        let date = Utc::now().format("%Y-%m-%d");
        let log_file = self.log_dir.join(format!("events-{date}.jsonl"));

        tokio::fs::create_dir_all(&self.log_dir).await?;

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)
            .await?;

        use tokio::io::AsyncWriteExt;

        for entry in buffer.drain(..) {
            let line = serde_json::to_string(&entry)?;
            file.write_all(line.as_bytes()).await?;
            file.write_all(b"\n").await?;
        }

        file.sync_all().await?;

        Ok(())
    }

    /// 脱敏处理事件数据
    fn sanitize_event_data(&self, event_type: &EventType) -> serde_json::Value {
        match event_type {
            EventType::ToolCall { name, args } => {
                let mut sanitized = args.clone();
                self.sanitize_json_value(&mut sanitized);
                serde_json::json!({
                    "tool_name": name,
                    "args": sanitized,
                })
            }
            EventType::ToolResult {
                name,
                output,
                is_error,
            } => {
                let truncated = if output.len() > MAX_OUTPUT_BYTES {
                    format!(
                        "{}... [truncated {} bytes]",
                        &output[..MAX_OUTPUT_BYTES],
                        output.len() - MAX_OUTPUT_BYTES
                    )
                } else {
                    output.clone()
                };
                serde_json::json!({
                    "tool_name": name,
                    "output": truncated,
                    "is_error": is_error,
                })
            }
            EventType::ModelRequest {
                model,
                prompt_tokens,
            } => {
                serde_json::json!({
                    "model": model,
                    "prompt_tokens": prompt_tokens,
                })
            }
            EventType::ModelResponse {
                model,
                completion_tokens,
            } => {
                serde_json::json!({
                    "model": model,
                    "completion_tokens": completion_tokens,
                })
            }
            EventType::Error { source, message } => {
                serde_json::json!({
                    "source": source,
                    "message": message,
                })
            }
            EventType::Decision { description } => {
                serde_json::json!({
                    "description": description,
                })
            }
        }
    }

    /// 递归脱敏 JSON 值
    fn sanitize_json_value(&self, value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                let keys: Vec<String> = map.keys().cloned().collect();
                for key in keys {
                    if Self::is_sensitive_key(&key) {
                        if let Some(val) = map.get_mut(&key) {
                            *val = serde_json::json!("[REDACTED]");
                        }
                    } else if let Some(val) = map.get_mut(&key) {
                        self.sanitize_json_value(val);
                    }
                }
            }
            serde_json::Value::Array(arr) => {
                for item in arr {
                    self.sanitize_json_value(item);
                }
            }
            _ => {}
        }
    }

    /// 判断 key 是否敏感
    fn is_sensitive_key(key: &str) -> bool {
        let lower = key.to_lowercase();
        lower.contains("key")
            || lower.contains("token")
            || lower.contains("secret")
            || lower.contains("password")
            || lower.contains("credential")
            || lower.contains("api_key")
            || lower.contains("apikey")
    }
}

impl Default for EventLogger {
    fn default() -> Self {
        Self::new(PathBuf::from(".synthia/logs"), DEFAULT_BATCH_SIZE)
    }
}
