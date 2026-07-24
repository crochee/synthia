use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 单次 API 调用的上下文追踪记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextTrace {
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 会话 ID
    pub session_id: String,
    /// 当前步骤/阶段
    pub stage: String,
    /// 消息数量
    pub message_count: usize,
    /// 总 token 数
    pub total_tokens: usize,
    /// 前缀 hash（用于 cache 追踪）
    pub prefix_hash: String,
    /// 是否触发了 pruning
    pub pruning_triggered: bool,
    /// pruning 阶段（如果触发）
    pub pruning_stage: Option<String>,
    /// 额外元数据
    pub metadata: serde_json::Value,
}

impl ContextTrace {
    /// 创建新的追踪记录
    pub fn new(
        session_id: String,
        stage: String,
        message_count: usize,
        total_tokens: usize,
        prefix_hash: String,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            session_id,
            stage,
            message_count,
            total_tokens,
            prefix_hash,
            pruning_triggered: false,
            pruning_stage: None,
            metadata: serde_json::json!({}),
        }
    }

    /// 记录 pruning 触发
    pub fn with_pruning(mut self, stage: &str) -> Self {
        self.pruning_triggered = true;
        self.pruning_stage = Some(stage.to_string());
        self
    }

    /// 添加元数据
    pub fn with_metadata(
        mut self,
        key: &str,
        value: serde_json::Value,
    ) -> Self {
        if let serde_json::Value::Object(mut map) = self.metadata {
            map.insert(key.to_string(), value);
            self.metadata = serde_json::Value::Object(map);
        }
        self
    }

    /// 保存为 JSON 文件
    pub async fn save_to(&self, trace_dir: &PathBuf) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(trace_dir).await?;

        let filename = format!(
            "trace-{}-{}.json",
            self.timestamp.format("%Y%m%d-%H%M%S"),
            self.session_id
        );
        let path = trace_dir.join(filename);

        let json = serde_json::to_string_pretty(self)?;
        tokio::fs::write(&path, json).await?;

        Ok(())
    }
}

/// 前缀稳定性追踪器，用于统计 prefix_stability_ratio
#[derive(Debug, Clone)]
pub struct PrefixStabilityTracker {
    previous_prefix_hash: Option<String>,
    total_calls: u64,
    stable_calls: u64,
}

impl PrefixStabilityTracker {
    /// 创建新的追踪器
    pub fn new() -> Self {
        Self {
            previous_prefix_hash: None,
            total_calls: 0,
            stable_calls: 0,
        }
    }

    /// 记录一次 API 调用，返回前缀是否稳定
    pub fn record_call(&mut self, current_prefix_hash: &str) -> bool {
        self.total_calls += 1;

        let is_stable = self
            .previous_prefix_hash
            .as_ref()
            .is_some_and(|prev| prev == current_prefix_hash);

        if is_stable {
            self.stable_calls += 1;
        }

        self.previous_prefix_hash = Some(current_prefix_hash.to_string());

        is_stable
    }

    /// 计算前缀稳定性比例（0.0 - 1.0）
    pub fn stability_ratio(&self) -> f64 {
        if self.total_calls <= 1 {
            // 第一次调用没有比较基准，视为稳定
            1.0
        } else {
            self.stable_calls as f64 / (self.total_calls - 1) as f64
        }
    }

    /// 获取总调用次数
    pub fn total_calls(&self) -> u64 {
        self.total_calls
    }

    /// 获取稳定调用次数
    pub fn stable_calls(&self) -> u64 {
        self.stable_calls
    }
}

impl Default for PrefixStabilityTracker {
    fn default() -> Self {
        Self::new()
    }
}
