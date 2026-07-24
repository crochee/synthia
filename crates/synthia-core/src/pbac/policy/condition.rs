//! The [`Condition`] struct (a single named policy predicate
//! that can require confirmation) + the [`ConditionResult`]
//! enum (the three-valued outcome of a
//! [`super::ConditionDefinition::evaluate`] call).

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Condition {
    pub name: String,
    pub description: String,
    pub requires_confirmation: bool,
    pub confirmation_message: Option<String>,
}

impl Condition {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            requires_confirmation: false,
            confirmation_message: None,
        }
    }

    pub fn with_confirmation(mut self, message: &str) -> Self {
        self.requires_confirmation = true;
        self.confirmation_message = Some(message.to_string());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionResult {
    Allowed,
    Denied(String),
    Indeterminate(String),
}

impl ConditionResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, ConditionResult::Allowed)
    }

    pub fn is_denied(&self) -> bool {
        matches!(self, ConditionResult::Denied(_))
    }

    pub fn is_indeterminate(&self) -> bool {
        matches!(self, ConditionResult::Indeterminate(_))
    }
}
