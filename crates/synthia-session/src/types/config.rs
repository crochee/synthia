//! The [`SessionConfig`] struct — model + max-tokens per
//! session. Defaults to `gpt-4o` / 4096.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub model: String,
    pub max_tokens: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            model: "gpt-4o".to_string(),
            max_tokens: 4096,
        }
    }
}
