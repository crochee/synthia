use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub id: String,
    pub name: String,
    pub description: String,
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub output: String,
    pub continue_session: bool,
}

impl CommandResult {
    pub fn new(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            continue_session: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandContext {
    pub session_id: String,
    pub workspace_root: PathBuf,
}

impl CommandContext {
    pub fn new(session_id: String, workspace_root: PathBuf) -> Self {
        Self {
            session_id,
            workspace_root,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_creation() {
        let cmd = Command {
            id: "1".to_string(),
            name: "help".to_string(),
            description: "Show help".to_string(),
            patterns: vec!["/help".to_string()],
        };
        assert_eq!(cmd.name, "help");
    }

    #[test]
    fn test_command_result() {
        let result = CommandResult::new("output");
        assert_eq!(result.output, "output");
        assert!(result.continue_session);
    }
}
