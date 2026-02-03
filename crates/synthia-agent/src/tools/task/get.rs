use async_trait::async_trait;
use rmcp::model::CallToolResult;
use schemars::schema_for;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    file_store::TaskFileStore,
    shared::{err_result, ok_result, parse_args},
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
    async fn test_task_get_tool_name() {
        let tool = crate::tools::task::TaskGetTool::new();
        assert_eq!(tool.name(), "task_get");
    }

    #[tokio::test]
    async fn test_task_get_tool_description() {
        let tool = crate::tools::task::TaskGetTool::new();
        assert!(!tool.description().is_empty());
    }

    #[tokio::test]
    async fn test_task_get_tool_parameters() {
        let tool = crate::tools::task::TaskGetTool::new();
        let params = tool.parameters();
        assert!(params.is_object());
    }

    #[tokio::test]
    async fn test_task_get_tool_call_success() {
        let store = make_store();
        let task = crate::tools::task::Task::new("test-task-1", "Test Task")
            .with_description("A test task");
        store.create_task(&task).await.unwrap();

        let tool = crate::tools::task::TaskGetTool { store };
        let args = serde_json::json!({ "task_id": "test-task-1" });
        let result = tool.call(args).await;

        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("test-task-1"));
        assert!(text.text.contains("Test Task"));
    }

    #[tokio::test]
    async fn test_task_get_tool_call_not_found() {
        let store = make_store();
        let tool = crate::tools::task::TaskGetTool { store };
        let args = serde_json::json!({ "task_id": "nonexistent" });
        let result = tool.call(args).await;

        assert!(result.is_error == Some(true));
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("task not found"));
    }

    #[tokio::test]
    async fn test_task_get_tool_call_invalid_args() {
        let store = make_store();
        let tool = crate::tools::task::TaskGetTool { store };
        let args = serde_json::json!({ "invalid_field": "value" });
        let result = tool.call(args).await;

        assert!(result.is_error == Some(true));
    }

    #[tokio::test]
    async fn test_task_get_tool_default() {
        let tool = crate::tools::task::TaskGetTool::default();
        assert_eq!(tool.name(), "task_get");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct GetRequest {
    task_id: String,
}

#[derive(Clone)]
pub struct TaskGetTool {
    store: TaskFileStore,
}

impl TaskGetTool {
    pub fn new() -> Self {
        Self {
            store: TaskFileStore::new(),
        }
    }
}

impl Default for TaskGetTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TaskGetTool {
    fn name(&self) -> &str {
        "task_get"
    }

    fn description(&self) -> &str {
        "Get task details by ID."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schema_for!(GetRequest)).unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let req: GetRequest = match parse_args(args) {
            Ok(r) => r,
            Err(e) => return e,
        };

        match self.store.get_task(&req.task_id).await {
            Ok(task) => ok_result(&task),
            Err(e) => err_result(format!("task not found: {e}")),
        }
    }
}
