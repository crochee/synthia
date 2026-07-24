use tokio::task::JoinHandle;

/// Watches for agent completion and emits a notification.
/// Uses detached tokio::spawn - caller doesn't need to await.
pub struct CompletionWatcher {
    _handle: JoinHandle<()>,
}

impl CompletionWatcher {
    /// Spawn a watcher that monitors the agent and emits completion notification.
    pub fn spawn<F, Fut>(on_complete: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let handle = tokio::spawn(async move {
            on_complete().await;
        });
        Self { _handle: handle }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_completion_watcher_spawns() {
        use std::sync::Arc;

        use tokio::sync::Mutex;

        let completed = Arc::new(Mutex::new(false));
        let completed_clone = completed.clone();
        let _watcher = CompletionWatcher::spawn(move || async move {
            let mut c = completed_clone.lock().await;
            *c = true;
        });
        // Give it a moment to run
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(*completed.lock().await);
    }
}
