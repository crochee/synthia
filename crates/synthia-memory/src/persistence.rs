use synthia_core::Error;
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time::{Duration, interval},
};

use crate::hot::HotMemory;

const DEFAULT_FLUSH_INTERVAL_SECS: u64 = 30;

/// Configuration for the persistence layer.
#[derive(Debug, Clone)]
pub struct PersistenceConfig {
    pub flush_interval_secs: u64,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            flush_interval_secs: DEFAULT_FLUSH_INTERVAL_SECS,
        }
    }
}

/// Signal sent to the background persistence task.
enum PersistenceSignal {
    /// Flush all dirty entries immediately.
    Flush,
    /// Flush remaining dirty entries and shut down.
    Stop,
}

/// MemoryPersistence manages periodic flushing of dirty HotMemory entries to disk.
///
/// It runs a background tokio task that:
/// - Checks for dirty entries on a configurable interval
/// - Flushes only modified entries to disk
/// - Responds to on-demand flush signals via a channel
/// - Performs a final flush on shutdown
pub struct MemoryPersistence {
    hot_memory: HotMemory,
    signal_tx: mpsc::Sender<PersistenceSignal>,
    task_handle: Option<JoinHandle<()>>,
}

impl MemoryPersistence {
    /// Creates a new MemoryPersistence instance and starts the background flush task.
    pub fn new(hot_memory: HotMemory, config: PersistenceConfig) -> Self {
        let (signal_tx, signal_rx) = mpsc::channel::<PersistenceSignal>(32);

        let task_handle = tokio::spawn(Self::background_flush(
            hot_memory.clone(),
            config,
            signal_rx,
        ));

        Self {
            hot_memory,
            signal_tx,
            task_handle: Some(task_handle),
        }
    }

    /// Creates a MemoryPersistence with default configuration.
    pub fn with_defaults(hot_memory: HotMemory) -> Self {
        Self::new(hot_memory, PersistenceConfig::default())
    }

    /// Sends a flush-on-demand signal to the background task.
    /// This triggers an immediate flush of all dirty entries.
    pub async fn flush_on_demand(&self) -> Result<(), Error> {
        self.signal_tx
            .send(PersistenceSignal::Flush)
            .await
            .map_err(|_| Error::Internal("Signal channel closed".into()))?;
        Ok(())
    }

    /// Sends a stop signal to the background task, which will flush all remaining
    /// dirty entries and then exit. Returns after the background task completes.
    pub async fn flush_on_stop(&mut self) -> Result<(), Error> {
        self.signal_tx
            .send(PersistenceSignal::Stop)
            .await
            .map_err(|_| Error::Internal("Signal channel closed".into()))?;

        if let Some(handle) = self.task_handle.take() {
            handle.await.map_err(|e| {
                Error::Internal(format!("Background task join failed: {}", e))
            })?;
        }

        tracing::info!(
            "MemoryPersistence: background task stopped, final flush complete"
        );
        Ok(())
    }

    /// The background flush task.
    async fn background_flush(
        hot_memory: HotMemory,
        config: PersistenceConfig,
        mut signal_rx: mpsc::Receiver<PersistenceSignal>,
    ) {
        let mut flush_interval =
            interval(Duration::from_secs(config.flush_interval_secs));

        tracing::info!(
            interval_secs = config.flush_interval_secs,
            "MemoryPersistence: background flush task started"
        );

        loop {
            tokio::select! {
                // Timed flush check
                _ = flush_interval.tick() => {
                    if hot_memory.has_dirty_entries().await {
                        tracing::debug!("MemoryPersistence: timed flush triggered");
                        if let Err(e) = hot_memory.flush_dirty().await {
                            tracing::error!(error = %e, "MemoryPersistence: timed flush failed");
                        }
                    }
                }

                // On-demand signal
                signal = signal_rx.recv() => {
                    match signal {
                        Some(PersistenceSignal::Flush) => {
                            tracing::debug!("MemoryPersistence: on-demand flush triggered");
                            if let Err(e) = hot_memory.flush_dirty().await {
                                tracing::error!(error = %e, "MemoryPersistence: on-demand flush failed");
                            }
                        }
                        Some(PersistenceSignal::Stop) => {
                            tracing::info!("MemoryPersistence: stop signal received, performing final flush");
                            if hot_memory.has_dirty_entries().await
                                && let Err(e) = hot_memory.flush_dirty().await
                            {
                                tracing::error!(error = %e, "MemoryPersistence: final flush failed");
                            }
                            break;
                        }
                        None => {
                            // Channel closed, perform final flush and exit
                            tracing::warn!("MemoryPersistence: signal channel closed, performing final flush");
                            if hot_memory.has_dirty_entries().await {
                                let _ = hot_memory.flush_dirty().await;
                            }
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Returns the hot memory instance.
    pub fn hot_memory(&self) -> &HotMemory {
        &self.hot_memory
    }
}

impl Drop for MemoryPersistence {
    fn drop(&mut self) {
        if self.task_handle.is_some() {
            tracing::warn!(
                "MemoryPersistence: dropped without calling flush_on_stop(), dirty entries may not be flushed"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::time::{Duration, sleep};

    use super::*;

    async fn create_test_persistence() -> (MemoryPersistence, tempfile::TempDir)
    {
        let temp_dir = tempfile::tempdir().unwrap();
        let hot_memory = HotMemory::new(temp_dir.path().to_path_buf());
        let config = PersistenceConfig {
            flush_interval_secs: 1,
        };
        let persistence = MemoryPersistence::new(hot_memory, config);
        (persistence, temp_dir)
    }

    #[tokio::test]
    async fn test_persistence_creates_background_task() {
        let (persistence, _temp_dir) = create_test_persistence().await;
        assert!(persistence.task_handle.is_some());
    }

    #[tokio::test]
    async fn test_flush_on_demand_writes_to_disk() {
        let (persistence, _temp_dir) = create_test_persistence().await;
        let hot = persistence.hot_memory();

        hot.write("test_key", "test_value").await.unwrap();

        persistence.flush_on_demand().await.unwrap();
        sleep(Duration::from_millis(100)).await;

        assert!(!hot.has_dirty_entries().await);
    }

    #[tokio::test]
    async fn test_flush_on_stop_flushes_remaining_entries() {
        let (mut persistence, _temp_dir) = create_test_persistence().await;
        let hot = persistence.hot_memory();

        // Write an entry
        hot.write("stop_test", "stop_value").await.unwrap();

        // Flush on stop should perform final flush
        persistence.flush_on_stop().await.unwrap();

        // Task handle should be None after stop
        assert!(persistence.task_handle.is_none());
    }

    #[tokio::test]
    async fn test_timed_flush_clears_dirty_flags() {
        let (mut persistence, _temp_dir) = create_test_persistence().await;
        let hot = persistence.hot_memory();

        // Write an entry
        hot.write("timed_test", "timed_value").await.unwrap();
        assert!(hot.has_dirty_entries().await);

        // Wait for the interval to trigger (1 second)
        sleep(Duration::from_secs(2)).await;

        // After timed flush, dirty flags should be cleared
        assert!(!hot.has_dirty_entries().await);

        // Clean up
        let _ = persistence.flush_on_stop().await;
    }

    #[tokio::test]
    async fn test_no_flush_when_clean() {
        let (mut persistence, _temp_dir) = create_test_persistence().await;
        let hot = persistence.hot_memory();

        // Initially no dirty entries
        assert!(!hot.has_dirty_entries().await);

        // Flush should be a no-op
        persistence.flush_on_demand().await.unwrap();
        assert!(!hot.has_dirty_entries().await);

        let _ = persistence.flush_on_stop().await;
    }

    #[tokio::test]
    async fn test_persistence_config_default() {
        let config = PersistenceConfig::default();
        assert_eq!(config.flush_interval_secs, 30);
    }

    #[tokio::test]
    async fn test_persistence_with_defaults() {
        let temp_dir = tempfile::tempdir().unwrap();
        let hot_memory = HotMemory::new(temp_dir.path().to_path_buf());
        let mut persistence = MemoryPersistence::with_defaults(hot_memory);

        assert!(persistence.task_handle.is_some());

        // Clean up
        let _ = persistence.flush_on_stop().await;
    }

    #[tokio::test]
    async fn test_multiple_flushes() {
        let (mut persistence, _temp_dir) = create_test_persistence().await;
        let hot = persistence.hot_memory();

        hot.write("multi1", "value1").await.unwrap();
        persistence.flush_on_demand().await.unwrap();
        sleep(Duration::from_millis(100)).await;
        assert!(!hot.has_dirty_entries().await);

        hot.write("multi2", "value2").await.unwrap();
        assert!(hot.has_dirty_entries().await);
        persistence.flush_on_demand().await.unwrap();
        sleep(Duration::from_millis(100)).await;
        assert!(!hot.has_dirty_entries().await);

        let _ = persistence.flush_on_stop().await;
    }
}
