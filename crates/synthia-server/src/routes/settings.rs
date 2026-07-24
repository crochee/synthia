//! User-facing settings persistence.
//!
//! Stores per-session overrides for provider, model and API key
//! in a JSON file under the workspace so that values survive
//! page reloads (and the E2E test that asserts this).

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use synthia_core::ApiResponse;
use tokio::sync::RwLock;

use crate::state::AppState;

#[derive(Default, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub model: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        alias = "apiKey"
    )]
    pub api_key: Option<String>,
    /// Per-skill enable/disable flags, keyed by skill name.
    /// Names that are absent (or mapped to `true`) are treated as enabled;
    /// `false` marks the skill as disabled.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub skills: HashMap<String, bool>,
}

impl Settings {
    /// Returns the enabled flag for the given skill. Missing entries
    /// default to `true` (enabled) to preserve existing behaviour for
    /// skills that pre-date the disable feature.
    pub fn is_skill_enabled(&self, name: &str) -> bool {
        self.skills.get(name).copied().unwrap_or(true)
    }
}

#[derive(Default)]
pub struct SettingsStore {
    inner: RwLock<Settings>,
    path: Option<PathBuf>,
}

impl SettingsStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_path(path: PathBuf) -> Self {
        let initial = path
            .exists()
            .then(|| std::fs::read_to_string(&path).ok())
            .flatten()
            .and_then(|raw| serde_json::from_str::<Settings>(&raw).ok())
            .unwrap_or_default();
        Self {
            inner: RwLock::new(initial),
            path: Some(path),
        }
    }

    pub async fn snapshot(&self) -> Settings {
        self.inner.read().await.clone()
    }

    pub async fn replace(&self, next: Settings) {
        let mut guard = self.inner.write().await;
        *guard = next.clone();
        if let Some(path) = self.path.as_ref() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string_pretty(&next) {
                let _ = std::fs::write(path, json);
            }
        }
    }
}

/// GET /api/settings - Return the currently persisted settings.
pub async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<Settings>> {
    let snapshot = state.settings.snapshot().await;
    Json(ApiResponse::ok(snapshot))
}

/// PUT /api/settings - Replace the persisted settings.
pub async fn put_settings(
    State(state): State<Arc<AppState>>,
    Json(req): Json<Settings>,
) -> Json<ApiResponse<Settings>> {
    let next = Settings {
        provider: req.provider.clone().filter(|s| !s.is_empty()),
        model: req.model.clone().filter(|s| !s.is_empty()),
        api_key: req.api_key.clone().filter(|s| !s.is_empty()),
        skills: req.skills.clone(),
    };
    state.settings.replace(next.clone()).await;
    Json(ApiResponse::ok(next))
}
