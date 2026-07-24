//! The data model: [`PolicyDefinition`] +
//! [`PolicyEffect`] + [`PolicyTarget`] + [`SubjectTarget`] +
//! [`ResourceTarget`].
//!
//! These are pure data records (no methods); they describe
//! the static shape of a policy that lives in a
//! configuration file. The `Policy` trait (see
//! [`super::policy_trait`]) is implemented separately by the
//! struct that *evaluates* a `PolicyDefinition`.

use std::collections::HashMap;

use super::condition_definition::ConditionDefinition;

#[derive(Debug, Clone)]
pub struct PolicyDefinition {
    pub name: String,
    pub effect: PolicyEffect,
    pub target: PolicyTarget,
    pub conditions: Vec<ConditionDefinition>,
    pub priority: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyEffect {
    Permit,
    Deny,
}

#[derive(Debug, Clone)]
pub struct PolicyTarget {
    pub subjects: Option<Vec<SubjectTarget>>,
    pub resources: Option<Vec<ResourceTarget>>,
    pub actions: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct SubjectTarget {
    pub ids: Option<Vec<String>>,
    pub roles: Option<Vec<String>>,
    pub attributes: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone)]
pub struct ResourceTarget {
    pub names: Option<Vec<String>>,
    pub types: Option<Vec<String>>,
    pub attributes: Option<HashMap<String, serde_json::Value>>,
}
