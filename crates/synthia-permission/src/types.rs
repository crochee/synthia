use serde::Serialize;

use crate::level::Permission;

#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub tool_name: String,
    pub input: serde_json::Value,
    pub requires_permission: bool,
}

impl PermissionRequest {
    pub fn new(
        tool_name: String,
        input: serde_json::Value,
        requires_permission: bool,
    ) -> Self {
        Self {
            tool_name,
            input,
            requires_permission,
        }
    }

    pub fn outcome(&self, outcome: Permission) -> PermissionOutcome {
        PermissionOutcome {
            tool_name: self.tool_name.clone(),
            outcome,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PermissionOutcome {
    pub tool_name: String,
    pub outcome: Permission,
}
