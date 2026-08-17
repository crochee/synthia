//! TodoWrite tool implementation.
//!
//! Manages an in-memory task list with status transitions and optional
//! dependency tracking. The tool is stateless across sessions — every
//! invocation supplies the full desired todo list, mirroring the
//! Claude Code `TodoWrite` semantics.

use std::collections::HashMap;

use schemars_derive::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    traits::Tool,
    types::{Context, ToolOutput},
};

const MAX_TODO_ITEMS: usize = 20;
const MAX_IN_PROGRESS: usize = 1;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Default,
    JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(extend("additionalProperties" = false))]
pub struct TodoItem {
    #[schemars(description = "Stable task identifier.")]
    pub id: String,
    #[schemars(description = "Task description.")]
    pub content: String,
    pub status: TodoStatus,
    #[serde(default)]
    #[schemars(
        description = "Present-continuous forms shown when status is in_progress."
    )]
    pub active_form: Option<Vec<String>>,
    #[serde(default)]
    #[schemars(description = "IDs of tasks this one depends on.")]
    pub deps: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(extend("additionalProperties" = false))]
struct TodoWriteRequest {
    #[schemars(
        length(max = 20),
        description = "Full desired task list (replaces prior state)."
    )]
    todos: Vec<TodoItem>,
}

fn validate_todo_items(todos: &[TodoItem]) -> Result<(), String> {
    if todos.len() > MAX_TODO_ITEMS {
        return Err(format!(
            "Maximum {MAX_TODO_ITEMS} todo items allowed (got {})",
            todos.len()
        ));
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

const MARKER_COMPLETED: &str = "[done]";
const MARKER_IN_PROGRESS: &str = "[in_progress]";
const MARKER_PENDING: &str = "[ ]";
const MARKER_CANCELLED: &str = "[cancelled]";

fn marker(status: TodoStatus) -> &'static str {
    match status {
        TodoStatus::Completed => MARKER_COMPLETED,
        TodoStatus::InProgress => MARKER_IN_PROGRESS,
        TodoStatus::Pending => MARKER_PENDING,
        TodoStatus::Cancelled => MARKER_CANCELLED,
    }
}

fn resolve_deps<'a>(
    deps: &'a [String],
    id_to_content: &HashMap<&'a str, &'a str>,
) -> String {
    let resolved: Vec<&str> = deps
        .iter()
        .filter_map(|dep_id| id_to_content.get(dep_id.as_str()).copied())
        .collect();
    if resolved.is_empty() {
        deps.join(", ")
    } else {
        resolved.join(", ")
    }
}

fn render(todos: &[TodoItem]) -> String {
    if todos.is_empty() {
        return "No todos.".to_string();
    }

    let id_to_content: HashMap<&str, &str> = todos
        .iter()
        .map(|item| (item.id.as_str(), item.content.as_str()))
        .collect();

    let mut lines = Vec::with_capacity(todos.len() + 4);
    lines.push("# Task List".to_string());
    lines.push(String::new());

    for item in todos {
        let mut line =
            format!("{} {} {}", item.id, marker(item.status), item.content);

        if item.status == TodoStatus::InProgress
            && let Some(active_forms) = item.active_form.as_deref()
            && !active_forms.is_empty()
        {
            line = format!(
                "{} {} {} <- {}",
                item.id,
                marker(item.status),
                item.content,
                active_forms.join(", ")
            );
        }

        if let Some(deps) = item.deps.as_deref()
            && !deps.is_empty()
        {
            let dep_contents = resolve_deps(deps, &id_to_content);
            line.push_str(&format!(" (deps: {dep_contents})"));
        }

        lines.push(line);
    }

    lines.push(String::new());
    lines.push(render_progress(todos));
    lines.join("\n")
}

fn render_progress(todos: &[TodoItem]) -> String {
    let completed = todos
        .iter()
        .filter(|item| matches!(item.status, TodoStatus::Completed))
        .count();
    let total = todos.len();
    let percentage = if total == 0 {
        0.0
    } else {
        (completed as f64 / total as f64) * 100.0
    };
    format!("Progress: {completed}/{total} completed ({percentage:.1}%)")
}

/// `TodoWrite` — manage a structured task list with dependency tracking.
#[derive(Debug, Clone, Copy, Default)]
pub struct TodoWriteTool;

impl TodoWriteTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Tool for TodoWriteTool {
    fn name(&self) -> &str {
        "TodoWrite"
    }

    fn description(&self) -> &str {
        "Replace the current task list with the supplied items. Tasks are \
         rendered back as a checklist with progress. Max 20 items; only one \
         may be in_progress at a time."
    }

    fn parameters(&self) -> serde_json::Value {
        // Schema is generated from `TodoWriteRequest` / `TodoItem`
        // / `TodoStatus` via `schemars`, so the type and the
        // LLM-facing schema cannot drift — including
        // `additionalProperties: false`, the `todos` `maxItems`,
        // and the `TodoStatus` enum values.
        serde_json::to_value(schemars::schema_for!(TodoWriteRequest))
            .expect("TodoWriteRequest schema is always serializable")
    }

    async fn call(
        &self,
        input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        let request: TodoWriteRequest = match serde_json::from_value(input) {
            Ok(r) => r,
            Err(e) => {
                return ToolOutput::error(format!("Invalid arguments: {e}"));
            }
        };

        if let Err(msg) = validate_todo_items(&request.todos) {
            return ToolOutput::error(msg);
        }

        ToolOutput::text(render(&request.todos))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn make_context() -> Context {
        Context::new("s1".to_string(), std::path::PathBuf::from("/tmp"))
    }

    fn item(id: &str, content: &str, status: TodoStatus) -> TodoItem {
        TodoItem {
            id: id.to_string(),
            content: content.to_string(),
            status,
            active_form: None,
            deps: None,
        }
    }

    // ---- validate_todo_items ---------------------------------------

    #[test]
    fn validate_accepts_empty_list() {
        assert!(validate_todo_items(&[]).is_ok());
    }

    #[test]
    fn validate_accepts_max_items() {
        let todos: Vec<TodoItem> = (0..MAX_TODO_ITEMS)
            .map(|i| item(&format!("t-{i}"), "Task", TodoStatus::Pending))
            .collect();
        assert!(validate_todo_items(&todos).is_ok());
    }

    #[test]
    fn validate_rejects_over_max() {
        let todos: Vec<TodoItem> = (0..(MAX_TODO_ITEMS + 1))
            .map(|i| item(&format!("t-{i}"), "Task", TodoStatus::Pending))
            .collect();
        let err = validate_todo_items(&todos).unwrap_err();
        assert!(err.contains(&MAX_TODO_ITEMS.to_string()));
    }

    #[test]
    fn validate_accepts_single_in_progress() {
        let todos = vec![
            item("a", "Done", TodoStatus::Completed),
            item("b", "Active", TodoStatus::InProgress),
            item("c", "Pending", TodoStatus::Pending),
        ];
        assert!(validate_todo_items(&todos).is_ok());
    }

    #[test]
    fn validate_rejects_multiple_in_progress() {
        let todos = vec![
            item("a", "First", TodoStatus::InProgress),
            item("b", "Second", TodoStatus::InProgress),
        ];
        let err = validate_todo_items(&todos).unwrap_err();
        assert!(err.contains("in_progress"));
    }

    // ---- render ----------------------------------------------------

    #[test]
    fn render_empty_returns_placeholder() {
        assert_eq!(render(&[]), "No todos.");
    }

    #[test]
    fn render_includes_progress_summary_and_markers() {
        let todos = vec![
            item("t-1", "Done task", TodoStatus::Completed),
            item("t-2", "Active task", TodoStatus::InProgress),
            item("t-3", "Pending task", TodoStatus::Pending),
            item("t-4", "Cancelled task", TodoStatus::Cancelled),
        ];
        let out = render(&todos);
        assert!(out.contains("# Task List"));
        assert!(out.contains(MARKER_COMPLETED));
        assert!(out.contains(MARKER_IN_PROGRESS));
        assert!(out.contains(MARKER_PENDING));
        assert!(out.contains(MARKER_CANCELLED));
        assert!(out.contains("Progress: 1/4 completed"));
        assert!(out.contains("(25.0%)"));
    }

    #[test]
    fn render_resolves_dependency_ids_to_content() {
        let todos = vec![
            item("a", "First", TodoStatus::Completed),
            TodoItem {
                deps: Some(vec!["a".to_string()]),
                ..item("b", "Second", TodoStatus::Pending)
            },
        ];
        let out = render(&todos);
        assert!(out.contains("(deps: First)"));
    }

    #[test]
    fn render_falls_back_to_ids_for_unknown_deps() {
        let todos = vec![TodoItem {
            deps: Some(vec!["missing".to_string()]),
            ..item("a", "Task", TodoStatus::Pending)
        }];
        let out = render(&todos);
        assert!(out.contains("(deps: missing)"));
    }

    #[test]
    fn render_includes_active_form_for_in_progress() {
        let todos = vec![TodoItem {
            active_form: Some(vec!["Working on it".to_string()]),
            ..item("a", "Build feature", TodoStatus::InProgress)
        }];
        let out = render(&todos);
        assert!(out.contains("<- Working on it"));
    }

    #[test]
    fn render_progress_zero_total_is_zero_percent() {
        assert_eq!(render_progress(&[]), "Progress: 0/0 completed (0.0%)");
    }

    // ---- TodoWriteTool --------------------------------------------

    #[test]
    fn tool_metadata_matches_contract() {
        let tool = TodoWriteTool::new();
        assert_eq!(tool.name(), "TodoWrite");
        assert!(tool.description().contains("task list"));
        let schema = tool.parameters();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["required"][0], "todos");

        let todos = &schema["properties"]["todos"];
        assert_eq!(todos["type"], "array");
        assert_eq!(
            todos["maxItems"], MAX_TODO_ITEMS,
            "todos schema maxItems must match runtime cap"
        );

        // `TodoStatus` is referenced from `TodoItem.status` via
        // `$defs/TodoStatus` (schemars 1.x follows draft 2020-12).
        // Pin the four enum values so a typo or rename fails here.
        let defs = schema["$defs"].as_object().expect("$defs");
        let todo_status_def =
            defs["TodoStatus"].as_object().expect("TodoStatus def");
        assert_eq!(todo_status_def["type"], "string");
        let status_enum: Vec<&str> = todo_status_def["enum"]
            .as_array()
            .expect("status enum")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(
            status_enum,
            vec!["pending", "in_progress", "completed", "cancelled"]
        );

        assert_eq!(
            schema["additionalProperties"], false,
            "additional fields must be rejected to match serde_json::from_value"
        );
    }

    #[tokio::test]
    async fn call_rejects_malformed_input() {
        let tool = TodoWriteTool::new();
        let out = tool
            .call(json!({"todos": "not-an-array"}), &make_context())
            .await;
        assert!(out.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn call_rejects_too_many_items() {
        let tool = TodoWriteTool::new();
        let todos: Vec<serde_json::Value> = (0..(MAX_TODO_ITEMS + 1))
            .map(|i| {
                json!({
                    "id": format!("t-{i}"),
                    "content": "Task",
                    "status": "pending"
                })
            })
            .collect();
        let out = tool.call(json!({"todos": todos}), &make_context()).await;
        assert!(out.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn call_rejects_multiple_in_progress() {
        let tool = TodoWriteTool::new();
        let out = tool
            .call(
                json!({
                    "todos": [
                        {"id": "a", "content": "First", "status": "in_progress"},
                        {"id": "b", "content": "Second", "status": "in_progress"}
                    ]
                }),
                &make_context(),
            )
            .await;
        assert!(out.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn call_renders_valid_list() {
        let tool = TodoWriteTool::new();
        let out = tool
            .call(
                json!({
                    "todos": [
                        {"id": "t-1", "content": "First", "status": "completed"},
                        {"id": "t-2", "content": "Second", "status": "pending"}
                    ]
                }),
                &make_context(),
            )
            .await;
        assert!(out.is_error.is_none());
        let text = match &out.content[0] {
            synthia_provider::types::ContentPart::Text(t) => t.text.clone(),
            _ => panic!("expected text content"),
        };
        assert!(text.contains("# Task List"));
        assert!(text.contains(MARKER_COMPLETED));
        assert!(text.contains(MARKER_PENDING));
        assert!(text.contains("Progress: 1/2 completed"));
    }

    #[tokio::test]
    async fn call_renders_empty_list() {
        let tool = TodoWriteTool::new();
        let out = tool.call(json!({"todos": []}), &make_context()).await;
        assert!(out.is_error.is_none());
        let text = match &out.content[0] {
            synthia_provider::types::ContentPart::Text(t) => t.text.clone(),
            _ => panic!("expected text content"),
        };
        assert!(text.contains("No todos"));
    }
}
