//! TodoWrite tool implementation
//!
//! This tool helps create and manage a structured task list for coding sessions.

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde_json::Value;

use super::{
    render::TodoRenderer,
    types::{TodoItem, TodoStatus, TodoWriteRequest},
};
use crate::tools::Tool;

const MAX_TODO_ITEMS: usize = 20;
const MAX_IN_PROGRESS: usize = 1;

fn validate_todo_items(todos: &[TodoItem]) -> Result<(), String> {
    if todos.len() > MAX_TODO_ITEMS {
        return Err(format!("Maximum {MAX_TODO_ITEMS} todo items allowed"));
    }
    let in_progress_count = todos
        .iter()
        .filter(|t| t.status == TodoStatus::InProgress)
        .count();
    if in_progress_count > MAX_IN_PROGRESS {
        return Err("Only one in_progress todo item allowed".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TodoWriteTool {}

impl TodoWriteTool {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl Tool for TodoWriteTool {
    fn name(&self) -> &str {
        "TodoWrite"
    }

    fn description(&self) -> &str {
        "Manage task list with dependency tracking."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 20,
                    "description": "Tasks (max 20, only one in_progress allowed)",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {"type": "string", "description": "ID"},
                            "content": {"type": "string", "description": "Description"},
                            "status": {"type": "string", "enum": ["pending", "in_progress", "completed", "cancelled"]},
                            "deps": {"type": "array", "items": {"type": "string"}, "description": "Dependencies"}
                        },
                        "required": ["id", "content", "status"]
                    }
                }
            },
            "required": ["todos"]
        })
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: TodoWriteRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid arguments: {e}"
                ))]);
            }
        };

        if let Err(msg) = validate_todo_items(&request.todos) {
            return CallToolResult::error(vec![Content::text(msg)]);
        }

        let renderer = TodoRenderer::new(&request.todos);
        let output = renderer.render();

        CallToolResult::success(vec![Content::text(output)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_todo_item(
        id: &str,
        content: &str,
        status: TodoStatus,
    ) -> TodoItem {
        TodoItem {
            id: id.to_string(),
            content: content.to_string(),
            status,
            active_form: None,
            deps: None,
        }
    }

    // =====================================================================
    // validate_todo_items tests
    // =====================================================================

    #[test]
    fn test_validate_todo_items_empty() {
        let todos: [TodoItem; 0] = [];
        assert!(validate_todo_items(&todos).is_ok());
    }

    #[test]
    fn test_validate_todo_items_single_item() {
        let todos =
            vec![create_todo_item("task-1", "Test task", TodoStatus::Pending)];
        assert!(validate_todo_items(&todos).is_ok());
    }

    #[test]
    fn test_validate_todo_items_max_items() {
        let todos: Vec<TodoItem> = (0..20)
            .map(|i| {
                create_todo_item(
                    &format!("task-{i}"),
                    "Task",
                    TodoStatus::Pending,
                )
            })
            .collect();
        assert!(validate_todo_items(&todos).is_ok());
    }

    #[test]
    fn test_validate_todo_items_exceeds_max() {
        let todos: Vec<TodoItem> = (0..21)
            .map(|i| {
                create_todo_item(
                    &format!("task-{i}"),
                    "Task",
                    TodoStatus::Pending,
                )
            })
            .collect();
        let result = validate_todo_items(&todos);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("20"));
    }

    #[test]
    fn test_validate_todo_items_single_in_progress() {
        let todos = vec![
            create_todo_item("task-1", "First", TodoStatus::Completed),
            create_todo_item("task-2", "Second", TodoStatus::InProgress),
        ];
        assert!(validate_todo_items(&todos).is_ok());
    }

    #[test]
    fn test_validate_todo_items_multiple_in_progress_fails() {
        let todos = vec![
            create_todo_item("task-1", "First", TodoStatus::InProgress),
            create_todo_item("task-2", "Second", TodoStatus::InProgress),
        ];
        let result = validate_todo_items(&todos);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("in_progress"));
    }

    #[test]
    fn test_validate_todo_items_mixed_statuses() {
        let todos = vec![
            create_todo_item("task-1", "Done", TodoStatus::Completed),
            create_todo_item("task-2", "Active", TodoStatus::InProgress),
            create_todo_item("task-3", "Pending", TodoStatus::Pending),
            create_todo_item("task-4", "Cancelled", TodoStatus::Cancelled),
        ];
        assert!(validate_todo_items(&todos).is_ok());
    }

    // =====================================================================
    // TodoWriteTool tests
    // =====================================================================

    #[test]
    fn test_todo_write_tool_new() {
        let tool = TodoWriteTool::new();
        assert_eq!(tool.name(), "TodoWrite");
    }

    #[test]
    fn test_todo_write_tool_name() {
        let tool = TodoWriteTool::new();
        assert_eq!(tool.name(), "TodoWrite");
    }

    #[test]
    fn test_todo_write_tool_description() {
        let tool = TodoWriteTool::new();
        assert_eq!(
            tool.description(),
            "Manage task list with dependency tracking."
        );
    }

    #[test]
    fn test_todo_write_tool_parameters() {
        let tool = TodoWriteTool::new();
        let params = tool.parameters();

        assert!(params.is_object());
        let obj = params.as_object().unwrap();
        assert_eq!(obj.get("type").unwrap(), "object");

        let properties = obj.get("properties").unwrap().as_object().unwrap();
        assert!(properties.contains_key("todos"));

        let todos_schema =
            properties.get("todos").unwrap().as_object().unwrap();
        assert_eq!(todos_schema.get("type").unwrap(), "array");
        assert_eq!(todos_schema.get("maxItems").unwrap(), 20);
    }

    #[test]
    fn test_todo_write_tool_call_invalid_args() {
        let tool = TodoWriteTool::new();
        let result = tokio_test::block_on(
            tool.call(serde_json::json!({"todos": "not-an-array"})),
        );

        assert!(result.is_error == Some(true));
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("Invalid arguments"));
    }

    #[test]
    fn test_todo_write_tool_call_too_many_items() {
        let tool = TodoWriteTool::new();
        let todos: Vec<serde_json::Value> = (0..21)
            .map(|i| {
                serde_json::json!({
                    "id": format!("task-{}", i),
                    "content": "Task",
                    "status": "pending"
                })
            })
            .collect();

        let result = tokio_test::block_on(
            tool.call(serde_json::json!({"todos": todos})),
        );

        assert!(result.is_error == Some(true));
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("20"));
    }

    #[test]
    fn test_todo_write_tool_call_multiple_in_progress() {
        let tool = TodoWriteTool::new();
        let todos = serde_json::json!({
            "todos": [
                {"id": "task-1", "content": "First", "status": "in_progress"},
                {"id": "task-2", "content": "Second", "status": "in_progress"}
            ]
        });

        let result = tokio_test::block_on(tool.call(todos));

        assert!(result.is_error == Some(true));
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("in_progress"));
    }

    #[test]
    fn test_todo_write_tool_call_success() {
        let tool = TodoWriteTool::new();
        let todos = serde_json::json!({
            "todos": [
                {"id": "task-1", "content": "First task", "status": "completed"},
                {"id": "task-2", "content": "Second task", "status": "pending"}
            ]
        });

        let result = tokio_test::block_on(tool.call(todos));

        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("# Task List"));
        assert!(text.text.contains("[✔]"));
        assert!(text.text.contains("[ ]"));
        assert!(text.text.contains("Progress: 1/2"));
    }

    #[test]
    fn test_todo_write_tool_call_empty_todos() {
        let tool = TodoWriteTool::new();
        let todos = serde_json::json!({
            "todos": []
        });

        let result = tokio_test::block_on(tool.call(todos));

        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("No todos"));
    }
}
