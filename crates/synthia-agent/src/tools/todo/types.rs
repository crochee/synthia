//! Todo types module
//!
//! Provides data types for the TodoWrite tool.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub(crate) enum TodoStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TodoItem {
    pub(crate) id: String,
    pub(crate) content: String,
    pub(crate) status: TodoStatus,
    #[serde(default)]
    pub(crate) active_form: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) deps: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TodoWriteRequest {
    pub(crate) todos: Vec<TodoItem>,
}
