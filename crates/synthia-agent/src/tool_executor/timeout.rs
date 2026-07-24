use std::{future::Future, time::Duration};

use tokio::time::timeout;

/// 工具执行超时错误
#[derive(Debug, thiserror::Error)]
pub enum TimeoutError {
    #[error("Tool execution timed out after {0:?}")]
    TimedOut(Duration),
    #[error("Tool execution failed: {0}")]
    Failed(String),
}

/// 带超时执行 future
pub async fn execute_with_timeout<F, T>(
    future: F,
    timeout_secs: u64,
) -> Result<T, TimeoutError>
where
    F: Future<Output = T>,
{
    match timeout(Duration::from_secs(timeout_secs), future).await {
        Ok(result) => Ok(result),
        Err(_) => {
            Err(TimeoutError::TimedOut(Duration::from_secs(timeout_secs)))
        }
    }
}

/// 带重试和退避的执行
pub async fn execute_with_retry<F, Fut, T, E>(
    mut operation: F,
    max_retries: u32,
    base_delay_secs: u64,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut last_error = None;

    for attempt in 0..=max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = Some(e);
                if attempt < max_retries {
                    let delay = Duration::from_secs(
                        base_delay_secs * 2u64.pow(attempt),
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    Err(last_error.unwrap())
}
