use std::{future::Future, time::Duration};

use tracing::warn;

use crate::task::types::TaskResult;

/// Execute a single task with timeout using tokio::time::timeout.
///
/// This is a standalone helper for cases where you don't need the
/// full scheduler but want timeout semantics.
pub async fn execute_with_timeout<F>(future: F, timeout: Duration) -> TaskResult
where
    F: Future<Output = TaskResult> + Send,
{
    match tokio::time::timeout(timeout, future).await {
        Ok(result) => result,
        Err(_) => {
            warn!(timeout_ms = timeout.as_millis(), "Task execution timed out");
            TaskResult::timeout()
        }
    }
}
