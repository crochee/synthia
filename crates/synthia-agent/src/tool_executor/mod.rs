#![allow(deprecated)]

pub mod config;
pub mod timeout;
pub mod truncate;

use std::{future::Future, time::Duration};

use config::{ToolCategoryTimeout, ToolExecutorConfig};
use timeout::{TimeoutError, execute_with_timeout};
use truncate::{TruncatedResult, truncate_result};

/// ToolExecutor 负责工具执行的超时、重试和截断
#[deprecated(
    note = "Use ToolOrchestrator instead; this type will be removed in a future release."
)]
pub struct ToolExecutor {
    config: ToolExecutorConfig,
    categories: ToolCategoryTimeout,
}

impl ToolExecutor {
    pub fn new(
        config: ToolExecutorConfig,
        categories: ToolCategoryTimeout,
    ) -> Self {
        Self { config, categories }
    }

    /// 获取工具类别的超时时间
    pub fn get_timeout_for_category(&self, category: &str) -> u64 {
        match category {
            "fs" => self.categories.fs_timeout_secs,
            "shell" => self.categories.shell_timeout_secs,
            "web" => self.categories.web_timeout_secs,
            "subagent" => self.categories.subagent_timeout_secs,
            "mcp" => self.categories.mcp_timeout_secs,
            _ => self.config.default_timeout_secs,
        }
    }

    /// 执行工具调用，带超时和重试
    pub async fn execute<F, Fut, T>(
        &self,
        category: &str,
        mut operation: F,
    ) -> Result<T, TimeoutError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, String>>,
    {
        let timeout_secs = self.get_timeout_for_category(category);
        let max_retries = self.config.max_retries;
        let retry_base_secs = self.config.retry_base_secs;

        let mut last_error = None;

        for attempt in 0..=max_retries {
            let result = execute_with_timeout(operation(), timeout_secs)
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r);

            match result {
                Ok(value) => return Ok(value),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < max_retries {
                        let delay = Duration::from_secs(
                            retry_base_secs * 2u64.pow(attempt),
                        );
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        Err(TimeoutError::Failed(last_error.unwrap()))
    }

    /// 截断工具执行结果
    pub fn truncate_output(&self, content: &str) -> TruncatedResult {
        truncate_result(
            content,
            self.config.truncate_threshold_bytes,
            self.config.truncate_head_bytes,
            self.config.truncate_tail_bytes,
        )
    }
}

impl Default for ToolExecutor {
    fn default() -> Self {
        Self::new(
            ToolExecutorConfig::default(),
            ToolCategoryTimeout::default(),
        )
    }
}
