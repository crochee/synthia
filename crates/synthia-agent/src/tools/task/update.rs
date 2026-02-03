use async_trait::async_trait;
use rmcp::model::CallToolResult;
use schemars::schema_for;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    data::{TaskPatch, TaskPriority, TaskStatus},
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
    async fn test_task_update_tool_name() {
        let tool = crate::tools::task::TaskUpdateTool::new();
        assert_eq!(tool.name(), "task_update");
    }

    #[tokio::test]
    async fn test_task_update_tool_description() {
        let tool = crate::tools::task::TaskUpdateTool::new();
        assert!(!tool.description().is_empty());
    }

    #[tokio::test]
    async fn test_task_update_tool_parameters() {
        let tool = crate::tools::task::TaskUpdateTool::new();
        let params = tool.parameters();
        assert!(params.is_object());
    }

    #[tokio::test]
    async fn test_task_update_tool_call_update_status() {
        let store = make_store();
        let task = crate::tools::task::Task::new("task-1", "Test Task");
        store.create_task(&task).await.unwrap();

        let tool = crate::tools::task::TaskUpdateTool { store };
        let args = serde_json::json!({
            "task_id": "task-1",
            "status": "in_progress"
        });
        let result = tool.call(args).await;

        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("in_progress"));
    }

    #[tokio::test]
    async fn test_task_update_tool_call_update_subject() {
        let store = make_store();
        let task = crate::tools::task::Task::new("task-1", "Original Subject");
        store.create_task(&task).await.unwrap();

        let tool = crate::tools::task::TaskUpdateTool { store };
        let args = serde_json::json!({
            "task_id": "task-1",
            "subject": "Updated Subject"
        });
        let result = tool.call(args).await;

        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("Updated Subject"));
    }

    #[tokio::test]
    async fn test_task_update_tool_call_update_description() {
        let store = make_store();
        let task = crate::tools::task::Task::new("task-1", "Test Task");
        store.create_task(&task).await.unwrap();

        let tool = crate::tools::task::TaskUpdateTool { store };
        let args = serde_json::json!({
            "task_id": "task-1",
            "description": "New description"
        });
        let result = tool.call(args).await;

        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("New description"));
    }

    #[tokio::test]
    async fn test_task_update_tool_call_update_owner() {
        let store = make_store();
        let task = crate::tools::task::Task::new("task-1", "Test Task");
        store.create_task(&task).await.unwrap();

        let tool = crate::tools::task::TaskUpdateTool { store };
        let args = serde_json::json!({
            "task_id": "task-1",
            "owner": "alice"
        });
        let result = tool.call(args).await;

        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("alice"));
    }

    #[tokio::test]
    async fn test_task_update_tool_call_update_priority() {
        let store = make_store();
        let task = crate::tools::task::Task::new("task-1", "Test Task");
        store.create_task(&task).await.unwrap();

        let tool = crate::tools::task::TaskUpdateTool { store };
        let args = serde_json::json!({
            "task_id": "task-1",
            "priority": "high"
        });
        let result = tool.call(args).await;

        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("high"));
    }

    #[tokio::test]
    async fn test_task_update_tool_call_update_deadline() {
        let store = make_store();
        let task = crate::tools::task::Task::new("task-1", "Test Task");
        store.create_task(&task).await.unwrap();

        let tool = crate::tools::task::TaskUpdateTool { store };
        let args = serde_json::json!({
            "task_id": "task-1",
            "deadline": 1735689600
        });
        let result = tool.call(args).await;

        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("1735689600"));
    }

    #[tokio::test]
    async fn test_task_update_tool_call_add_blocked_by() {
        let store = make_store();

        let dep_task =
            crate::tools::task::Task::new("dep-task", "Dependency Task");
        store.create_task(&dep_task).await.unwrap();

        let task = crate::tools::task::Task::new("task-1", "Test Task");
        store.create_task(&task).await.unwrap();

        let tool = crate::tools::task::TaskUpdateTool { store };
        let args = serde_json::json!({
            "task_id": "task-1",
            "add_blocked_by": ["dep-task"]
        });
        let result = tool.call(args).await;

        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("dep-task"));
    }

    #[tokio::test]
    async fn test_task_update_tool_call_add_blocks() {
        let store = make_store();

        let task = crate::tools::task::Task::new("task-1", "Test Task");
        store.create_task(&task).await.unwrap();

        let blocked_task =
            crate::tools::task::Task::new("blocked-task", "Blocked Task");
        store.create_task(&blocked_task).await.unwrap();

        let tool = crate::tools::task::TaskUpdateTool {
            store: store.clone(),
        };
        let args = serde_json::json!({
            "task_id": "task-1",
            "add_blocks": ["blocked-task"]
        });
        let result = tool.call(args).await;

        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("blocked-task"));
    }

    #[tokio::test]
    async fn test_task_update_tool_call_complete_unblocks() {
        let store = make_store();

        let dep_task =
            crate::tools::task::Task::new("dep-task", "Dependency Task");
        store.create_task(&dep_task).await.unwrap();

        let mut blocked_task =
            crate::tools::task::Task::new("blocked-task", "Blocked Task");
        blocked_task.blocked_by = vec!["dep-task".to_string()];
        store.create_task(&blocked_task).await.unwrap();

        let tool = crate::tools::task::TaskUpdateTool {
            store: store.clone(),
        };
        let args = serde_json::json!({
            "task_id": "dep-task",
            "status": "completed"
        });
        let _result = tool.call(args).await;

        let _ = store.get_task("blocked-task").await;
    }

    #[tokio::test]
    async fn test_task_update_tool_call_not_found() {
        let store = make_store();
        let tool = crate::tools::task::TaskUpdateTool { store };
        let args = serde_json::json!({
            "task_id": "nonexistent",
            "status": "in_progress"
        });
        let result = tool.call(args).await;

        assert!(result.is_error == Some(true));
    }

    #[tokio::test]
    async fn test_task_update_tool_call_invalid_args() {
        let store = make_store();
        let tool = crate::tools::task::TaskUpdateTool { store };
        let args = serde_json::json!({ "invalid": "args" });
        let result = tool.call(args).await;

        assert!(result.is_error == Some(true));
    }

    #[tokio::test]
    async fn test_task_update_tool_default() {
        let tool = crate::tools::task::TaskUpdateTool::default();
        assert_eq!(tool.name(), "task_update");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct UpdateRequest {
    task_id: String,
    #[serde(default)]
    status: Option<TaskStatus>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    add_blocked_by: Option<Vec<String>>,
    #[serde(default)]
    add_blocks: Option<Vec<String>>,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    team_id: Option<String>,
    #[serde(default)]
    priority: Option<TaskPriority>,
    #[serde(default)]
    deadline: Option<i64>,
}

#[derive(Clone)]
pub struct TaskUpdateTool {
    store: TaskFileStore,
}

impl TaskUpdateTool {
    pub fn new() -> Self {
        Self {
            store: TaskFileStore::new(),
        }
    }

    async fn add_dependency(
        store: &TaskFileStore,
        task_id: &str,
        dep_id: &str,
        forward_field: &[&str],
        reverse_field: &[&str],
    ) -> Result<(), String> {
        let mut task = store
            .get_task(task_id)
            .await
            .map_err(|e| format!("failed to get task: {e}"))?;

        for field in forward_field {
            match *field {
                "blocked_by" => {
                    if !task.blocked_by.contains(&dep_id.to_string()) {
                        task.blocked_by.push(dep_id.to_string());
                    }
                }
                "blocks" => {
                    if !task.blocks.contains(&dep_id.to_string()) {
                        task.blocks.push(dep_id.to_string());
                    }
                }
                _ => return Err("invalid field".to_string()),
            }
        }

        store
            .update_task(
                task_id,
                &TaskPatch {
                    blocked_by: Some(task.blocked_by.clone()),
                    blocks: Some(task.blocks.clone()),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| format!("failed to update task: {e}"))?;

        let mut dep_task = store
            .get_task(dep_id)
            .await
            .map_err(|e| format!("failed to get dependency task: {e}"))?;

        for field in reverse_field {
            match *field {
                "blocked_by" => {
                    if !dep_task.blocked_by.contains(&task_id.to_string()) {
                        dep_task.blocked_by.push(task_id.to_string());
                    }
                }
                "blocks" => {
                    if !dep_task.blocks.contains(&task_id.to_string()) {
                        dep_task.blocks.push(task_id.to_string());
                    }
                }
                _ => return Err("invalid field".to_string()),
            }
        }

        store
            .update_task(
                dep_id,
                &TaskPatch {
                    blocked_by: Some(dep_task.blocked_by),
                    blocks: Some(dep_task.blocks),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| format!("failed to update dependency task: {e}"))?;

        Ok(())
    }

    async fn try_unblock_blocked_tasks(
        store: &TaskFileStore,
        completed_id: &str,
    ) {
        let tasks = match store.list_tasks().await {
            Ok(tasks) => tasks,
            Err(_) => return,
        };

        let pending_tasks: Vec<_> = tasks
            .into_iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .collect();

        for task in pending_tasks {
            if !task.blocked_by.iter().any(|id| id == completed_id) {
                continue;
            }

            let new_blocked_by: Vec<String> = task
                .blocked_by
                .iter()
                .filter(|id| *id != completed_id)
                .cloned()
                .collect();

            let patch = TaskPatch {
                blocked_by: Some(new_blocked_by),
                ..Default::default()
            };

            if let Err(e) = store.update_task(&task.id, &patch).await {
                tracing::warn!("failed to unblock task {}: {}", task.id, e);
            }
        }
    }
}

impl Default for TaskUpdateTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TaskUpdateTool {
    fn name(&self) -> &str {
        "task_update"
    }

    fn description(&self) -> &str {
        "Update task status, subject, description, dependencies, team assignment, priority, or deadline."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schema_for!(UpdateRequest)).unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let req: UpdateRequest = match parse_args(args) {
            Ok(r) => r,
            Err(e) => return e,
        };

        if let Some(ref add_blocked_by) = req.add_blocked_by {
            for dep_id in add_blocked_by {
                if let Err(e) = Self::add_dependency(
                    &self.store,
                    &req.task_id,
                    dep_id,
                    &["blocked_by"],
                    &["blocks"],
                )
                .await
                {
                    return err_result(e);
                }
            }
        }

        if let Some(ref add_blocks) = req.add_blocks {
            for dep_id in add_blocks {
                if let Err(e) = Self::add_dependency(
                    &self.store,
                    &req.task_id,
                    dep_id,
                    &["blocks"],
                    &["blocked_by"],
                )
                .await
                {
                    return err_result(e);
                }
            }
        }

        let current_task = match self.store.get_task(&req.task_id).await {
            Ok(t) => t,
            Err(e) => {
                return err_result(format!(
                    "failed to get task after deps: {e}"
                ));
            }
        };

        let patch = TaskPatch {
            subject: req.subject,
            description: req.description,
            status: req.status.or(Some(current_task.status)),
            blocked_by: Some(current_task.blocked_by),
            blocks: Some(current_task.blocks),
            owner: req.owner,
            team_id: req.team_id,
            priority: req.priority,
            deadline: req.deadline,
            output: None,
        };

        match self.store.update_task(&req.task_id, &patch).await {
            Ok(updated) => {
                if updated.status == TaskStatus::Completed {
                    Self::try_unblock_blocked_tasks(&self.store, &req.task_id)
                        .await;
                }
                ok_result(&updated)
            }
            Err(e) => err_result(format!("failed to update task: {e}")),
        }
    }
}
