//! Private debounce + atomic-swap coordinator used by
//! [`super::ConfigWatcher`].
//!
//! The watcher spawns a task that loops over a
//! `mpsc::Receiver<WatcherMessage>`; every time the OS
//! reports a file change the task sends
//! [`WatcherMessage::FileChanged`] which (eventually) calls
//! [`ReloadCoordinator::try_reload`]. The coordinator
//! debounces by [`DEFAULT_DEBOUNCE_WINDOW`] and atomically
//! swaps the shared config if [`HotReloadableFields::is_empty`]
//! returns `false`.

use std::{path::PathBuf, time::Duration};

use tokio::time::Instant;
use tracing::{error, info};

use super::types::{HotReloadableFields, SharedConfig, SynthiaConfig};

/// Default debounce window for file-change events.
///
/// Two edits within this window collapse to a single
/// reload.
pub(super) const DEFAULT_DEBOUNCE_WINDOW: Duration = Duration::from_millis(500);

pub(super) enum WatcherMessage {
    FileChanged,
}

/// Owns the debounce state for a single config file.
pub(super) struct ReloadCoordinator {
    pub(super) config_path: PathBuf,
    pub(super) shared_config: SharedConfig,
    pub(super) debounce_window: Duration,
    pub(super) last_reload: Instant,
}

impl ReloadCoordinator {
    pub(super) fn new(
        config_path: PathBuf,
        shared_config: SharedConfig,
    ) -> Self {
        Self {
            config_path,
            shared_config,
            debounce_window: DEFAULT_DEBOUNCE_WINDOW,
            last_reload: Instant::now() - DEFAULT_DEBOUNCE_WINDOW,
        }
    }

    /// Attempt to reload the config from disk.
    ///
    /// Returns the [`HotReloadableFields`] diff if a swap
    /// was performed, `None` if the change was suppressed
    /// (within the debounce window, the file is invalid, or
    /// no hot-reloadable fields differ).
    pub(super) async fn try_reload(&mut self) -> Option<HotReloadableFields> {
        if self.last_reload.elapsed() < self.debounce_window {
            return None;
        }

        let new_config = match SynthiaConfig::load_from_file(&self.config_path)
        {
            Ok(cfg) => cfg,
            Err(e) => {
                error!(
                    error = %e,
                    path = ?self.config_path,
                    "Config reload failed — retaining current config"
                );
                self.last_reload = Instant::now();
                return None;
            }
        };

        let old_config = self.shared_config.read().await.clone();
        let diff = HotReloadableFields::diff(&old_config, &new_config);

        if diff.is_empty() {
            info!(path = ?self.config_path, "Config file changed but no hot-reloadable fields differ");
            self.last_reload = Instant::now();
            return None;
        }

        let mut guard = self.shared_config.write().await;
        *guard = new_config.clone();
        drop(guard);

        self.last_reload = Instant::now();

        let changed = diff.changed_field_names();
        info!(fields = ?changed, "Config hot-reloaded successfully");

        Some(diff)
    }
}
