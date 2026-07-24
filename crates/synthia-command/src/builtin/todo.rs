use std::sync::Mutex;

use async_trait::async_trait;
use synthia_core::Error;

use crate::{
    traits::CommandHandler,
    types::{CommandContext, CommandResult},
};

#[derive(Debug, Clone)]
enum TodoStatus {
    Pending,
    Completed,
}

#[derive(Debug, Clone)]
struct TodoItem {
    id: usize,
    description: String,
    status: TodoStatus,
}

pub struct TodoCommand {
    items: Mutex<Vec<TodoItem>>,
    next_id: Mutex<usize>,
}

impl TodoCommand {
    pub fn new() -> Self {
        Self {
            items: Mutex::new(Vec::new()),
            next_id: Mutex::new(1),
        }
    }

    fn list_todos(&self) -> Result<CommandResult, Error> {
        let items = self.items.lock().unwrap();
        if items.is_empty() {
            Ok(CommandResult::new(
                "No tasks tracked. Use /todo add <description> to add a task.",
            ))
        } else {
            let mut output = String::from("Tasks:\n");
            for item in items.iter() {
                let status = match item.status {
                    TodoStatus::Pending => "[ ]",
                    TodoStatus::Completed => "[x]",
                };
                output.push_str(&format!(
                    "{} {}: {}\n",
                    status, item.id, item.description
                ));
            }
            Ok(CommandResult::new(output))
        }
    }

    fn add_todo(
        &self,
        description: Option<&str>,
    ) -> Result<CommandResult, Error> {
        match description {
            Some(desc) if !desc.is_empty() => {
                let mut next_id = self.next_id.lock().unwrap();
                let id = *next_id;
                *next_id += 1;

                let item = TodoItem {
                    id,
                    description: desc.to_string(),
                    status: TodoStatus::Pending,
                };

                self.items.lock().unwrap().push(item);
                Ok(CommandResult::new(format!("Added task #{}: {}", id, desc)))
            }
            _ => Ok(CommandResult::new("Usage: /todo add <task_description>")),
        }
    }

    fn complete_todo(
        &self,
        id_str: Option<&str>,
    ) -> Result<CommandResult, Error> {
        match id_str {
            Some(id_str) => {
                let id: usize = id_str.parse().map_err(|_| {
                    Error::Parse(format!("Invalid task ID: {}", id_str))
                })?;

                let mut items = self.items.lock().unwrap();
                if let Some(item) = items.iter_mut().find(|i| i.id == id) {
                    item.status = TodoStatus::Completed;
                    Ok(CommandResult::new(format!(
                        "Marked task #{} as complete.",
                        id
                    )))
                } else {
                    Ok(CommandResult::new(format!("Task #{} not found.", id)))
                }
            }
            None => Ok(CommandResult::new("Usage: /todo complete <task_id>")),
        }
    }
}

#[async_trait]
impl CommandHandler for TodoCommand {
    fn name(&self) -> &str {
        "todo"
    }

    async fn execute(
        &self,
        args: &str,
        _ctx: &CommandContext,
    ) -> Result<CommandResult, Error> {
        let args = args.trim();

        if args.is_empty() {
            return self.list_todos();
        }

        let parts: Vec<&str> = args.splitn(2, ' ').collect();
        let action = parts[0];

        match action {
            "add" => self.add_todo(parts.get(1).copied()),
            "complete" => self.complete_todo(parts.get(1).copied()),
            "list" => self.list_todos(),
            _ => Ok(CommandResult::new(
                "Usage: /todo [list|add|complete]\n\
                 /todo               - List all tasks\n\
                 /todo add <desc>    - Add a new task\n\
                 /todo complete <id> - Mark a task as complete",
            )),
        }
    }
}

impl Default for TodoCommand {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_todo_empty() {
        let cmd = TodoCommand::new();
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("", &ctx).await.unwrap();
        assert!(result.output.contains("No tasks"));
    }

    #[tokio::test]
    async fn test_todo_add() {
        let cmd = TodoCommand::new();
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("add Implement feature", &ctx).await.unwrap();
        assert!(result.output.contains("Added task"));
        assert!(result.output.contains("Implement feature"));
    }

    #[tokio::test]
    async fn test_todo_list_after_add() {
        let cmd = TodoCommand::new();
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        cmd.execute("add Task one", &ctx).await.unwrap();
        cmd.execute("add Task two", &ctx).await.unwrap();
        let result = cmd.execute("list", &ctx).await.unwrap();
        assert!(result.output.contains("Task one"));
        assert!(result.output.contains("Task two"));
        assert!(result.output.contains("[ ]"));
    }

    #[tokio::test]
    async fn test_todo_complete() {
        let cmd = TodoCommand::new();
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        cmd.execute("add Task to complete", &ctx).await.unwrap();
        let result = cmd.execute("complete 1", &ctx).await.unwrap();
        assert!(result.output.contains("complete"));
    }

    #[tokio::test]
    async fn test_todo_complete_invalid_id() {
        let cmd = TodoCommand::new();
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("complete 999", &ctx).await.unwrap();
        assert!(result.output.contains("not found"));
    }

    #[tokio::test]
    async fn test_todo_add_empty_description() {
        let cmd = TodoCommand::new();
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("add ", &ctx).await.unwrap();
        assert!(result.output.contains("Usage"));
    }

    #[tokio::test]
    async fn test_todo_unknown_action() {
        let cmd = TodoCommand::new();
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("unknown", &ctx).await.unwrap();
        assert!(result.output.contains("Usage"));
    }
}
