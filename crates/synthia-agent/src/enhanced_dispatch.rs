#![allow(deprecated)]

use std::{sync::Arc, time::Duration};

use synthia_provider::ToolUse;
use synthia_tool::{ToolOutput, ToolRegistry, types::ToolExecutionContext};
use tracing::{error, warn};

#[derive(Debug, Clone)]
pub struct DispatcherConfig {
    pub max_retries: u32,
    pub initial_retry_delay_ms: u64,
    pub max_retry_delay_ms: u64,
    pub timeout_ms: u64,
}

impl Default for DispatcherConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_retry_delay_ms: 500,
            max_retry_delay_ms: 10000,
            timeout_ms: 30000,
        }
    }
}

#[deprecated(
    note = "Use ToolOrchestrator instead; this type will be removed in a future release."
)]
pub struct EnhancedToolDispatcher {
    registry: Arc<ToolRegistry>,
    config: DispatcherConfig,
}

impl EnhancedToolDispatcher {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self {
            registry,
            config: DispatcherConfig::default(),
        }
    }

    pub fn with_config(mut self, config: DispatcherConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.config.max_retries = retries;
        self
    }

    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.config.timeout_ms = timeout_ms;
        self
    }

    fn is_retryable_error(output: &ToolOutput) -> bool {
        output.is_error.unwrap_or(false) && {
            let text = output
                .content
                .iter()
                .filter_map(|p| p.text())
                .collect::<String>();
            text.to_lowercase().contains("timeout")
                || text.to_lowercase().contains("rate limit")
                || text.to_lowercase().contains("temporary")
                || text.to_lowercase().contains("connection")
                || text.to_lowercase().contains("network")
        }
    }

    fn calculate_retry_delay(&self, attempt: u32) -> Duration {
        let base_delay = self.config.initial_retry_delay_ms;
        let max_delay = self.config.max_retry_delay_ms;
        let delay = base_delay * 2u64.pow(attempt.saturating_sub(1));
        Duration::from_millis(delay.min(max_delay))
    }

    pub async fn dispatch(&self, tool_use: ToolUse) -> ToolOutput {
        self.dispatch_with_context(
            tool_use,
            &ToolExecutionContext::new(
                "dispatcher".to_string(),
                std::path::PathBuf::from("."),
            ),
        )
        .await
    }

    pub async fn dispatch_with_context(
        &self,
        tool_use: ToolUse,
        context: &ToolExecutionContext,
    ) -> ToolOutput {
        let tool_name = tool_use.name.clone();
        let timeout = Duration::from_millis(self.config.timeout_ms);
        let mut attempt = 0u32;

        loop {
            attempt += 1;

            let output = tokio::time::timeout(
                timeout,
                self.registry
                    .run_with_context(vec![tool_use.clone()], context.clone()),
            )
            .await;

            match output {
                Ok(Ok(results)) => {
                    if let Some(result) = results.into_iter().next() {
                        if result.is_error.unwrap_or(false) {
                            if attempt >= self.config.max_retries
                                || !Self::is_retryable_error(&result)
                            {
                                return result;
                            }
                            let delay = self.calculate_retry_delay(attempt);
                            warn!(
                                tool = %tool_name,
                                attempt = attempt,
                                delay_ms = delay.as_millis(),
                                "Retrying tool call"
                            );
                            tokio::time::sleep(delay).await;
                        } else {
                            return result;
                        }
                    } else {
                        return ToolOutput::error(
                            "No output from tool execution",
                        );
                    }
                }
                Ok(Err(e)) => {
                    error!(
                        tool = %tool_name,
                        attempt = attempt,
                        error = %e,
                        "Tool execution failed"
                    );

                    if attempt >= self.config.max_retries {
                        return ToolOutput::error(format!(
                            "Tool '{}' failed after {} attempts: {}",
                            tool_name, attempt, e
                        ));
                    }

                    let delay = self.calculate_retry_delay(attempt);
                    warn!(
                        tool = %tool_name,
                        attempt = attempt,
                        delay_ms = delay.as_millis(),
                        "Retrying tool call"
                    );
                    tokio::time::sleep(delay).await;
                }
                Err(_) => {
                    if attempt >= self.config.max_retries {
                        return ToolOutput::error(format!(
                            "Tool '{}' timed out after {} attempts",
                            tool_name, attempt
                        ));
                    }

                    let delay = self.calculate_retry_delay(attempt);
                    warn!(
                        tool = %tool_name,
                        attempt = attempt,
                        delay_ms = delay.as_millis(),
                        "Tool timed out, retrying"
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    pub async fn dispatch_parallel(
        &self,
        tool_calls: Vec<ToolUse>,
        context: &ToolExecutionContext,
    ) -> Vec<ToolOutput> {
        if tool_calls.is_empty() {
            return Vec::new();
        }

        let semaphore = Arc::new(tokio::sync::Semaphore::new(10));
        let mut futures = Vec::with_capacity(tool_calls.len());

        for tool_use in tool_calls {
            let sem = Arc::clone(&semaphore);
            let dispatcher = self.clone();
            let context = context.clone();

            futures.push(tokio::spawn(async move {
                let _permit = sem.acquire().await;
                dispatcher.dispatch_with_context(tool_use, &context).await
            }));
        }

        let mut results = Vec::with_capacity(futures.len());
        for f in futures {
            match f.await {
                Ok(result) => results.push(result),
                Err(e) => results
                    .push(ToolOutput::error(format!("Task panicked: {}", e))),
            }
        }
        results
    }
}

impl Clone for EnhancedToolDispatcher {
    fn clone(&self) -> Self {
        Self {
            registry: Arc::clone(&self.registry),
            config: self.config.clone(),
        }
    }
}

#[cfg(test)]
#[allow(deprecated)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn test_dispatcher_config_default() {
        let config = DispatcherConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.timeout_ms, 30000);
    }

    #[test]
    fn test_is_retryable_error_timeout() {
        let output = ToolOutput::error("Request timeout");
        assert!(EnhancedToolDispatcher::is_retryable_error(&output));
    }

    #[test]
    fn test_is_retryable_error_rate_limit() {
        let output = ToolOutput::error("Rate limit exceeded");
        assert!(EnhancedToolDispatcher::is_retryable_error(&output));
    }

    #[test]
    fn test_is_not_retryable_error() {
        let output = ToolOutput::error("Tool not found");
        assert!(!EnhancedToolDispatcher::is_retryable_error(&output));
    }

    #[test]
    fn test_calculate_retry_delay() {
        let dispatcher = EnhancedToolDispatcher::new(Arc::new(
            synthia_tool::ToolRegistry::new(),
        ));

        assert_eq!(dispatcher.calculate_retry_delay(1).as_millis(), 500);
        assert_eq!(dispatcher.calculate_retry_delay(2).as_millis(), 1000);
        assert_eq!(dispatcher.calculate_retry_delay(3).as_millis(), 2000);
    }

    #[test]
    fn test_calculate_retry_delay_max() {
        let mut config = DispatcherConfig::default();
        config.max_retry_delay_ms = 3000;

        let dispatcher = EnhancedToolDispatcher::new(Arc::new(
            synthia_tool::ToolRegistry::new(),
        ))
        .with_config(config);

        assert_eq!(dispatcher.calculate_retry_delay(10).as_millis(), 3000);
    }
}
