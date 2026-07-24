//! `spawn` and `graceful_shutdown` helpers for the memory background task.

use std::{sync::Arc, time::Duration};

use anyhow::Result;
use synthia_memory::types::{MemoryEvent, MemoryStore};
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, error, info, warn};

use super::task::MemoryBackgroundTask;

const DEFAULT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

pub fn spawn<S: MemoryStore + 'static>(
    store: Arc<S>,
    shutdown_token: CancellationToken,
    channel_capacity: usize,
) -> (JoinHandle<Result<()>>, mpsc::Sender<MemoryEvent>) {
    let (tx, rx) = mpsc::channel(channel_capacity);
    let _shutdown_for_return = shutdown_token.clone();
    let span = tracing::info_span!("memory_background_task");

    let handle = tokio::spawn(
        async move {
            let mut task = MemoryBackgroundTask::new(store, rx, shutdown_token);
            task.run().await
        }
        .instrument(span),
    );

    (handle, tx)
}

pub async fn graceful_shutdown(
    handle: JoinHandle<Result<()>>,
    shutdown_token: CancellationToken,
    timeout: Duration,
) -> Result<()> {
    shutdown_token.cancel();

    match tokio::time::timeout(timeout, handle).await {
        Ok(Ok(Ok(()))) => {
            info!("Memory background task shut down gracefully");
            Ok(())
        }
        Ok(Ok(Err(e))) => {
            error!(error = %e, "Memory background task exited with error");
            Err(anyhow::anyhow!("Task error: {}", e))
        }
        Ok(Err(e)) => {
            error!(error = %e, "Memory background task panicked");
            Err(anyhow::anyhow!("Task panicked: {}", e))
        }
        Err(_) => {
            warn!("Memory background task shutdown timeout, force dropping");
            Ok(())
        }
    }
}

pub fn default_shutdown_timeout() -> Duration {
    DEFAULT_SHUTDOWN_TIMEOUT
}
