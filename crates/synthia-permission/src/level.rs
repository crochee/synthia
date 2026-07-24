use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Block,
    AutoApprove,
    RequireConfirm,
    RequireExplicit,
    Deny { reason: String },
}
