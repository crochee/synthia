use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{
    action::ActionAttributes,
    environment::EnvironmentAttributes,
    resource::ResourceAttributes,
    subject::SubjectAttributes,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessRequest {
    pub subject: SubjectAttributes,
    pub resource: ResourceAttributes,
    pub action: ActionAttributes,
    pub environment: EnvironmentAttributes,
    pub context: HashMap<String, serde_json::Value>,
}

impl AccessRequest {
    pub fn new(subject_id: &str, resource: &str, action: &str) -> Self {
        Self {
            subject: SubjectAttributes {
                id: subject_id.to_string(),
                ..Default::default()
            },
            resource: ResourceAttributes {
                name: resource.to_string(),
                ..Default::default()
            },
            action: ActionAttributes {
                name: action.to_string(),
                ..Default::default()
            },
            environment: EnvironmentAttributes::default(),
            context: HashMap::new(),
        }
    }

    pub fn with_subject(mut self, subject: SubjectAttributes) -> Self {
        self.subject = subject;
        self
    }

    pub fn with_resource(mut self, resource: ResourceAttributes) -> Self {
        self.resource = resource;
        self
    }

    pub fn with_action(mut self, action: ActionAttributes) -> Self {
        self.action = action;
        self
    }

    pub fn with_environment(
        mut self,
        environment: EnvironmentAttributes,
    ) -> Self {
        self.environment = environment;
        self
    }

    pub fn with_context(
        mut self,
        key: &str,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.context.insert(key.to_string(), value.into());
        self
    }

    pub fn get_subject_attr(&self, key: &str) -> Option<&serde_json::Value> {
        self.context.get(&format!("subject.{}", key))
    }

    pub fn get_resource_attr(&self, key: &str) -> Option<&serde_json::Value> {
        self.context.get(&format!("resource.{}", key))
    }
}
