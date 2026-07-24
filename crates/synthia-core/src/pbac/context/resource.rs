use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceAttributes {
    pub name: String,
    pub resource_type: Option<String>,
    pub owner: Option<String>,
    pub sensitivity_level: Option<u32>,
    pub attributes: HashMap<String, serde_json::Value>,
}

impl ResourceAttributes {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ..Default::default()
        }
    }

    pub fn resource_type(mut self, resource_type: &str) -> Self {
        self.resource_type = Some(resource_type.to_string());
        self
    }

    pub fn owner(mut self, owner: &str) -> Self {
        self.owner = Some(owner.to_string());
        self
    }

    pub fn sensitivity_level(mut self, level: u32) -> Self {
        self.sensitivity_level = Some(level);
        self
    }

    pub fn is_owned_by(&self, subject_id: &str) -> bool {
        self.owner
            .as_ref()
            .map(|o| o == subject_id)
            .unwrap_or(false)
    }
}
