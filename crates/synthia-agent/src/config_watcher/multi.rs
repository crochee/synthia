//! [`MultiConfigWatcher`] — manages one
//! [`ConfigWatcher`] per [`ConfigType`] and fans callbacks
//! out from a single registration point.
//!
//! Used by the runtime entry point to bootstrap the whole
//! config surface in one call: `MultiConfigWatcher::watch_all`
//! iterates a `Vec<(ConfigType, PathBuf, Callback)>` and
//! routes each entry to a dedicated watcher.

use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

use synthia_core::Error;
use tokio::sync::RwLock;

use super::{
    coordinator::DEFAULT_DEBOUNCE_WINDOW,
    types::{ConfigChangeCallback, ConfigType},
    watcher::ConfigWatcher,
};

pub struct MultiConfigWatcher {
    watchers: Arc<RwLock<HashMap<ConfigType, ConfigWatcher>>>,
    callbacks: Arc<RwLock<HashMap<ConfigType, Vec<ConfigChangeCallback>>>>,
    debounce_duration: Duration,
}

impl MultiConfigWatcher {
    pub fn new() -> Self {
        Self {
            watchers: Arc::new(RwLock::new(HashMap::new())),
            callbacks: Arc::new(RwLock::new(HashMap::new())),
            debounce_duration: DEFAULT_DEBOUNCE_WINDOW,
        }
    }

    pub fn with_debounce(mut self, duration: Duration) -> Self {
        self.debounce_duration = duration;
        self
    }

    pub async fn watch(
        &self,
        config_type: ConfigType,
        path: PathBuf,
        callback: ConfigChangeCallback,
    ) -> Result<(), Error> {
        let watcher = ConfigWatcher::new(&path).await?;

        let mut callbacks = self.callbacks.write().await;
        callbacks
            .entry(config_type)
            .or_insert_with(Vec::new)
            .push(callback);

        let mut watchers = self.watchers.write().await;
        watchers.insert(config_type, watcher);

        Ok(())
    }

    pub async fn watch_all(
        &self,
        configs: Vec<(ConfigType, PathBuf, ConfigChangeCallback)>,
    ) -> Result<(), Error> {
        for (config_type, path, callback) in configs {
            self.watch(config_type, path, callback).await?;
        }
        Ok(())
    }

    pub fn start(&self) -> Result<(), Error> {
        Ok(())
    }

    pub async fn shutdown(&mut self) {
        let mut watchers = self.watchers.write().await;
        for (_, mut watcher) in watchers.drain() {
            watcher.shutdown().await;
        }
    }
}

impl Default for MultiConfigWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for MultiConfigWatcher {
    fn clone(&self) -> Self {
        Self {
            watchers: Arc::clone(&self.watchers),
            callbacks: Arc::clone(&self.callbacks),
            debounce_duration: self.debounce_duration,
        }
    }
}
