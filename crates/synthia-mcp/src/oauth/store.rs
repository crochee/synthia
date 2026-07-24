//! The [`CredentialStore`] struct + 2 constructors
//! (`new` / `with_client`).
//!
//! The 6 simple storage accessors
//! ([`store_token`][Self::store_token] /
//! [`get_token`][Self::get_token] /
//! [`remove_token`][Self::remove_token] /
//! [`store_config`][Self::store_config] /
//! [`get_config`][Self::get_config] /
//! [`get_valid_token`][Self::get_valid_token]) and 3
//! persistence methods
//! ([`persist_to_disk`][Self::persist_to_disk] /
//! [`load_from_disk`][Self::load_from_disk] /
//! [`shutdown`][Self::shutdown]) live here. The 4
//! auth-flow methods are in [`super::flow`].

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use reqwest::Client as HttpClient;
use tokio::sync::RwLock;

use super::types::{OAuthConfig, OAuthToken};

pub struct CredentialStore {
    pub(super) tokens: Arc<RwLock<HashMap<String, OAuthToken>>>,
    pub(super) configs: Arc<RwLock<HashMap<String, OAuthConfig>>>,
    pub(super) storage_path: PathBuf,
    pub(super) http_client: HttpClient,
}

impl CredentialStore {
    pub fn new(storage_path: PathBuf) -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
            configs: Arc::new(RwLock::new(HashMap::new())),
            storage_path,
            http_client: HttpClient::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    pub fn with_client(storage_path: PathBuf, client: HttpClient) -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
            configs: Arc::new(RwLock::new(HashMap::new())),
            storage_path,
            http_client: client,
        }
    }

    pub async fn store_token(&self, server_name: &str, token: OAuthToken) {
        self.tokens
            .write()
            .await
            .insert(server_name.to_string(), token);
        let _ = self.persist_to_disk().await;
    }

    pub async fn get_token(&self, server_name: &str) -> Option<OAuthToken> {
        self.tokens.read().await.get(server_name).cloned()
    }

    pub async fn remove_token(&self, server_name: &str) {
        self.tokens.write().await.remove(server_name);
        let _ = self.persist_to_disk().await;
    }

    pub async fn store_config(&self, server_name: &str, config: OAuthConfig) {
        self.configs
            .write()
            .await
            .insert(server_name.to_string(), config);
    }

    pub async fn get_config(&self, server_name: &str) -> Option<OAuthConfig> {
        self.configs.read().await.get(server_name).cloned()
    }

    pub async fn get_valid_token(
        &self,
        server_name: &str,
    ) -> Option<OAuthToken> {
        let token = self.get_token(server_name).await?;
        if token.is_expired() {
            None
        } else {
            Some(token)
        }
    }

    pub(super) async fn persist_to_disk(&self) -> std::io::Result<()> {
        let tokens = self.tokens.read().await;
        let content = serde_json::to_string_pretty(&*tokens).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;
        std::fs::create_dir_all(&self.storage_path)?;
        std::fs::write(self.storage_path.join("mcp_tokens.json"), content)?;
        Ok(())
    }

    pub async fn load_from_disk(&self) -> std::io::Result<()> {
        let path = self.storage_path.join("mcp_tokens.json");
        if !path.exists() {
            return Ok(());
        }
        let content = std::fs::read_to_string(&path)?;
        let tokens: HashMap<String, OAuthToken> =
            serde_json::from_str(&content).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, e)
            })?;
        let mut store = self.tokens.write().await;
        *store = tokens;
        Ok(())
    }

    /// Save all credentials to disk before shutdown.
    pub async fn shutdown(&self) -> std::io::Result<()> {
        tracing::info!("Saving OAuth credentials to disk");
        self.persist_to_disk().await
    }
}
