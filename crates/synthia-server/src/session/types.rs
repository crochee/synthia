//! Session types for API responses

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub name: Option<String>,
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: Option<i64>,
    #[serde(default)]
    pub message_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    pub before_count: usize,
    pub after_count: usize,
    pub strategy: String,
    pub token_ratio_before: f64,
    pub token_ratio_after: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedMessage {
    pub role: String,
    pub content: String,
}
