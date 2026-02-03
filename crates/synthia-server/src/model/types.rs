//! Model types for API requests and responses

use serde::{Deserialize, Serialize};

use crate::config::ModelConfig;

#[derive(Debug, Deserialize)]
pub struct AddModelProviderRequest {
    pub name: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    #[serde(default)]
    pub models: Vec<ModelConfig>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateModelRequest {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub description: Option<String>,
    pub context_window: Option<usize>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub name: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    #[serde(default)]
    pub models: Vec<ModelConfig>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub provider: String,
    pub name: String,
    pub description: Option<String>,
    pub context_window: Option<usize>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}
