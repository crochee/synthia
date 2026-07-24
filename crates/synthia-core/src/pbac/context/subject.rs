use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubjectAttributes {
    pub id: String,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub roles: Vec<String>,
    pub groups: Vec<String>,
    pub clearance_level: Option<u32>,
    pub attributes: HashMap<String, serde_json::Value>,
}

impl SubjectAttributes {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            ..Default::default()
        }
    }

    pub fn user_id(mut self, user_id: &str) -> Self {
        self.user_id = Some(user_id.to_string());
        self
    }

    pub fn session_id(mut self, session_id: &str) -> Self {
        self.session_id = Some(session_id.to_string());
        self
    }

    pub fn role(mut self, role: &str) -> Self {
        self.roles.push(role.to_string());
        self
    }

    pub fn clearance_level(mut self, level: u32) -> Self {
        self.clearance_level = Some(level);
        self
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role || r == "*")
    }

    pub fn in_group(&self, group: &str) -> bool {
        self.groups.iter().any(|g| g == group || g == "*")
    }
}
