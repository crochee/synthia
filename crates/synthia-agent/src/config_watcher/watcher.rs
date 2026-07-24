//! [`ConfigWatcher`] — the public, file-watching wrapper
//! around [`super::coordinator::ReloadCoordinator`].
//!
//! Owns the `notify::RecommendedWatcher` and the spawned
//! debouncer task. `new` initializes both; `shutdown` sends
//! the cancellation signal to the task.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use notify::{
    Config as NotifyConfig,
    RecommendedWatcher,
    RecursiveMode,
    Watcher,
};
use synthia_core::Error;
use tokio::sync::{RwLock, mpsc};
use tracing::info;

use super::{
    coordinator::{DEFAULT_DEBOUNCE_WINDOW, ReloadCoordinator, WatcherMessage},
    types::{
        ConfigChangeCallback,
        HotReloadableFields,
        SharedConfig,
        SynthiaConfig,
    },
};

pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
    shared_config: SharedConfig,
    config_path: PathBuf,
    callbacks: Arc<RwLock<HashMap<PathBuf, Vec<ConfigChangeCallback>>>>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl ConfigWatcher {
    pub async fn new(config_path: &Path) -> Result<Self, Error> {
        let initial_config = SynthiaConfig::load_from_file(config_path)?;
        let shared_config = Arc::new(RwLock::new(initial_config));

        let config_path = config_path.to_path_buf();
        let (tx, mut rx) = mpsc::unbounded_channel::<WatcherMessage>();
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        let config_path_for_callback = config_path.clone();

        let coordinator = Arc::new(RwLock::new(ReloadCoordinator::new(
            config_path.clone(),
            shared_config.clone(),
        )));

        let callbacks: Arc<
            RwLock<HashMap<PathBuf, Vec<ConfigChangeCallback>>>,
        > = Arc::new(RwLock::new(HashMap::new()));
        let callbacks_clone = callbacks.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                    msg = rx.recv() => {
                        match msg {
                            Some(WatcherMessage::FileChanged) => {
                                if let Some(diff) = coordinator.write().await.try_reload().await {
                                    let names = diff.changed_field_names();
                                    info!(
                                        target: "synthia.telemetry",
                                        event = "config_reloaded",
                                        fields = ?names,
                                        timestamp = chrono::Utc::now().to_rfc3339(),
                                        "ConfigReloaded"
                                    );

                                    let cbs = callbacks_clone.read().await;
                                    if let Some(callbacks) = cbs.get(&config_path_for_callback) {
                                        for cb in callbacks {
                                            let value = serde_json::json!({
                                                "changed_fields": names,
                                                "config_type": "main"
                                            });
                                            cb(value).await;
                                        }
                                    }
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        let tx_clone = tx;
        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    use notify::EventKind;
                    match event.kind {
                        EventKind::Modify(_)
                        | EventKind::Create(_)
                        | EventKind::Remove(_) => {
                            let _ = tx_clone.send(WatcherMessage::FileChanged);
                        }
                        _ => {}
                    }
                }
            },
            NotifyConfig::default(),
        )
        .map_err(|e| {
            Error::ConfigWatcher(format!("watcher init failed: {e}"))
        })?;

        watcher
            .watch(&config_path, RecursiveMode::NonRecursive)
            .map_err(|e| {
                Error::ConfigWatcher(format!("watch setup failed: {e}"))
            })?;

        info!(path = ?config_path, "ConfigWatcher started");

        Ok(Self {
            _watcher: watcher,
            shared_config,
            config_path,
            callbacks,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    pub fn shared_config(&self) -> SharedConfig {
        self.shared_config.clone()
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub async fn reload(&self) -> Result<HotReloadableFields, String> {
        let new_config = SynthiaConfig::load_from_file(&self.config_path)
            .map_err(|e| format!("Failed to load config: {e}"))?;

        let old_config = self.shared_config.read().await.clone();
        let diff = HotReloadableFields::diff(&old_config, &new_config);

        if diff.is_empty() {
            return Err("No hot-reloadable fields have changed".to_string());
        }

        let mut guard = self.shared_config.write().await;
        *guard = new_config;
        drop(guard);

        let changed = diff.changed_field_names();
        info!(
            target: "synthia.telemetry",
            event = "config_reloaded",
            fields = ?changed,
            timestamp = chrono::Utc::now().to_rfc3339(),
            "ConfigReloaded"
        );

        Ok(diff)
    }

    pub fn with_debounce(self, _duration: Duration) -> Self {
        // Note: the per-watcher debounce currently comes from
        // the spawned coordinator's `DEFAULT_DEBOUNCE_WINDOW`.
        // This setter is a no-op kept for API stability.
        let _ = DEFAULT_DEBOUNCE_WINDOW;
        self
    }

    pub async fn watch(&self, path: PathBuf, callback: ConfigChangeCallback) {
        let mut callbacks = self.callbacks.write().await;
        callbacks
            .entry(path.clone())
            .or_insert_with(Vec::new)
            .push(callback);
    }

    pub async fn watch_all(
        &self,
        configs: Vec<(PathBuf, ConfigChangeCallback)>,
    ) {
        let mut callbacks = self.callbacks.write().await;
        for (path, callback) in configs {
            callbacks
                .entry(path)
                .or_insert_with(Vec::new)
                .push(callback);
        }
    }

    pub fn start(&self) -> Result<(), Error> {
        Ok(())
    }

    pub async fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
    }
}
