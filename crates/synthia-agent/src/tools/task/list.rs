use async_trait::async_trait;
use rmcp::model::CallToolResult;
use schemars::schema_for;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    data::TaskStatus,
    file_store::TaskFileStore,
    shared::{TaskFilter, err_result, filter_tasks, ok_result, parse_args},
};
use crate::tools::Tool;

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::tools::Tool;

    fn make_store() -> crate::tools::task::TaskFileStore {
        let dir = tempdir().unwrap();
        crate::tools::task::TaskFileStore::with_base(dir.path().to_path_buf())
    }

    #[tokio::test]
    async fn test_task_list_tool_name() {
        let tool = crate::tools::task::TaskListTool::new();
        assert_eq!(tool.name(), "task_list");
    }

    #[tokio::test]
    async fn test_task_list_tool_description() {
        let tool = crate::tools::task::TaskListTool::new();
        assert!(!tool.description().is_empty());
    }

    #[tokio::test]
    async fn test_task_list_tool_parameters() {
        let tool = crate::tools::task::TaskListTool::new();
        let params = tool.parameters();
        assert!(params.is_object());
    }

    #[tokio::test]
    async fn test_task_list_tool_call_empty() {
        let store = make_store();
        let tool = crate::tools::task::TaskListTool { store };
        let args = serde_json::json!({});
        let result = tool.call(args).await;

        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("[]") || text.text.contains("[]"));
    }

    #[tokio::test]
    async fn test_task_list_tool_call_with_tasks() {
        let store = make_store();

        let task1 = crate::tools::task::Task::new("task-1", "Task 1");
        let task2 = crate::tools::task::Task::new("task-2", "Task 2");
        store.create_task(&task1).await.unwrap();
        store.create_task(&task2).await.unwrap();

        let tool = crate::tools::task::TaskListTool { store };
        let args = serde_json::json!({});
        let result = tool.call(args).await;

        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("task-1") && text.text.contains("task-2"));
    }

    #[tokio::test]
    async fn test_task_list_tool_call_filter_by_status() {
        let store = make_store();

        let task1 = crate::tools::task::Task::new("task-1", "Task 1");
        let mut task2 = crate::tools::task::Task::new("task-2", "Task 2");
        task2.status = crate::tools::task::TaskStatus::InProgress;

        store.create_task(&task1).await.unwrap();
        store.create_task(&task2).await.unwrap();

        let tool = crate::tools::task::TaskListTool { store };
        let args = serde_json::json!({ "status": "in_progress" });
        let result = tool.call(args).await;

        let text = result.content[0].as_text().unwrap();
        assert!(!text.text.is_empty());
    }

    #[tokio::test]
    async fn test_task_list_tool_call_filter_by_owner() {
        let store = make_store();

        let task1 = crate::tools::task::Task::new("task-1", "Task 1")
            .with_owner("alice");
        let task2 =
            crate::tools::task::Task::new("task-2", "Task 2").with_owner("bob");

        store.create_task(&task1).await.unwrap();
        store.create_task(&task2).await.unwrap();

        let tool = crate::tools::task::TaskListTool { store };
        let args = serde_json::json!({ "owner": "alice" });
        let result = tool.call(args).await;

        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("task-1"));
        assert!(!text.text.contains("task-2") || text.text.contains("alice"));
    }

    #[tokio::test]
    async fn test_task_list_tool_default() {
        let tool = crate::tools::task::TaskListTool::default();
        assert_eq!(tool.name(), "task_list");
    }
}

#[derive(Debug, Default, Serialize, Deserialize, schemars::JsonSchema)]
struct ListRequest {
    #[serde(default)]
    status: Option<TaskStatus>,
    #[serde(default)]
    owner: Option<String>,
}

#[derive(Clone)]
pub struct TaskListTool {
    store: TaskFileStore,
}

impl TaskListTool {
    pub fn new() -> Self {
        Self {
            store: TaskFileStore::new(),
        }
    }
}

impl Default for TaskListTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TaskListTool {
    fn name(&self) -> &str {
        "task_list"
    }

    fn description(&self) -> &str {
        "List all tasks. Filter by status or owner."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schema_for!(ListRequest)).unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let req: ListRequest = match parse_args(args) {
            Ok(r) => r,
            Err(e) => return e,
        };

        let filter = TaskFilter {
            status: req.status,
            owner: req.owner,
        };

        match filter_tasks(&self.store, filter).await {
            Ok(tasks) => ok_result(&tasks),
            Err(e) => err_result(e),
        }
    }
}
