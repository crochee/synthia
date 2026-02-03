//! Interaction types
//!
//! Shared types for interaction tools.

use serde::{Deserialize, Serialize};

/// Question option for user interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    /// Option label
    pub label: String,
    /// Option description
    #[serde(default)]
    pub description: String,
}

/// Question for user interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    /// Question text
    pub question: String,
    /// Question header
    #[serde(default)]
    pub header: String,
    /// Available options
    pub options: Vec<QuestionOption>,
    /// Whether multiple selection is allowed
    #[serde(default)]
    pub multi_select: bool,
}

/// Response from user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionResponse {
    /// Request ID
    pub request_id: String,
    /// User answers
    pub answers: Vec<QuestionAnswer>,
}

/// User answer to a question
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionAnswer {
    /// Selected options
    pub selected: Vec<String>,
    /// Other text input
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other: Option<String>,
}

/// Request to ask user questions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionRequest {
    /// Request ID
    pub id: String,
    /// Tool call ID
    pub tool_call_id: String,
    /// Questions to ask
    pub questions: Vec<Question>,
}
