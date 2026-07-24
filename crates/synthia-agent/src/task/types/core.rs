use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Structured context attached to a dispatched sub-task.
///
/// Contains the task description, referenced files, code snippets,
/// and constraints that get injected into the sub-agent prompt.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TaskContext {
    /// High-level description of what the task should achieve.
    pub description: String,
    /// File paths the sub-task should be aware of (resolved at dispatch time).
    pub file_references: Vec<String>,
    /// Inline code snippets that provide additional context.
    pub code_snippets: Vec<CodeSnippet>,
    /// Constraints the sub-task must respect (e.g., "don't modify tests").
    pub constraints: Vec<String>,
}

impl TaskContext {
    pub fn new(description: String) -> Self {
        Self {
            description,
            file_references: Vec::new(),
            code_snippets: Vec::new(),
            constraints: Vec::new(),
        }
    }

    pub fn with_files(mut self, files: Vec<String>) -> Self {
        self.file_references = files;
        self
    }

    pub fn with_snippets(mut self, snippets: Vec<CodeSnippet>) -> Self {
        self.code_snippets = snippets;
        self
    }

    pub fn with_constraints(mut self, constraints: Vec<String>) -> Self {
        self.constraints = constraints;
        self
    }
}

/// A named code snippet injected as task context.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CodeSnippet {
    pub name: String,
    pub content: String,
}

impl CodeSnippet {
    pub fn new(name: String, content: String) -> Self {
        Self { name, content }
    }
}

/// Priority level for a dispatched task.
///
/// High-priority tasks are scheduled before Normal and Low tasks.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize,
)]
pub enum TaskPriority {
    High = 2,
    #[default]
    Medium = 1,
    Low = 0,
}

impl TaskPriority {
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }

    pub fn is_higher_than(&self, other: &Self) -> bool {
        self.as_u8() > other.as_u8()
    }
}

/// Status of a completed task execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Success,
    Error,
    Timeout,
}

/// Structured result from a dispatched sub-task.
///
/// Replaces the legacy plain-string return type, providing
/// exit codes, status, and artifact file paths.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskResult {
    /// Combined stdout/stderr output from the task.
    pub output: String,
    /// Whether the task succeeded, errored, or timed out.
    pub status: TaskStatus,
    /// Exit code from the underlying process, if any.
    pub exit_code: Option<i32>,
    /// File paths of artifacts produced by the task.
    pub artifacts: Vec<String>,
}

impl TaskResult {
    pub fn success(output: String) -> Self {
        Self {
            output,
            status: TaskStatus::Success,
            exit_code: Some(0),
            artifacts: Vec::new(),
        }
    }

    pub fn error(output: String) -> Self {
        Self {
            output,
            status: TaskStatus::Error,
            exit_code: Some(1),
            artifacts: Vec::new(),
        }
    }

    pub fn timeout() -> Self {
        Self {
            output: "Task timed out".to_string(),
            status: TaskStatus::Timeout,
            exit_code: None,
            artifacts: Vec::new(),
        }
    }

    pub fn with_exit_code(mut self, code: i32) -> Self {
        self.exit_code = Some(code);
        self
    }

    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts = artifacts;
        self
    }

    pub fn is_success(&self) -> bool {
        self.status == TaskStatus::Success
    }
}

/// Default timeout for task execution (30 seconds).
pub const DEFAULT_TASK_TIMEOUT: Duration = Duration::from_secs(30);
