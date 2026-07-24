use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Notification {
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
