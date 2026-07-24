use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionAttributes {
    pub name: String,
    pub action_type: Option<String>,
    pub attributes: HashMap<String, serde_json::Value>,
}

impl ActionAttributes {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ..Default::default()
        }
    }

    pub fn action_type(mut self, action_type: &str) -> Self {
        self.action_type = Some(action_type.to_string());
        self
    }

    pub fn is_read(&self) -> bool {
        self.action_type
            .as_ref()
            .map(|t| t == "read")
            .unwrap_or(false)
            || self.name.contains("read")
            || self.name.contains("get")
            || self.name.contains("list")
    }

    pub fn is_write(&self) -> bool {
        self.action_type
            .as_ref()
            .map(|t| t == "write")
            .unwrap_or(false)
            || self.name.contains("write")
            || self.name.contains("create")
            || self.name.contains("edit")
    }

    pub fn is_delete(&self) -> bool {
        self.action_type
            .as_ref()
            .map(|t| t == "delete")
            .unwrap_or(false)
            || self.name.contains("delete")
            || self.name.contains("remove")
    }

    pub fn is_execute(&self) -> bool {
        self.action_type
            .as_ref()
            .map(|t| t == "execute")
            .unwrap_or(false)
            || self.name.contains("bash")
            || self.name.contains("exec")
            || self.name.contains("run")
    }
}
