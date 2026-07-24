use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StructuredOutput {
    pub key: String,
    pub value: serde_json::Value,
}
