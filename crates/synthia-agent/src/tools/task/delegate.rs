use async_trait::async_trait;
use rmcp::model::CallToolResult;
use schemars::schema_for;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    data::{TaskPatch, TaskStatus},
    file_store::TaskFileStore,
    shared::{err_result, ok_result, parse_args},
};
use crate::tools::{Tool, team::file_store::TeamStorage};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct DelegateRequest {
    task_id: String,
    assignee: String,
}

#[derive(Clone)]
pub struct TaskDelegateTool {
    task_store: TaskFileStore,
    team_storage: TeamStorage,
}

impl TaskDelegateTool {
    pub fn new() -> Self {
        let team_storage = TeamStorage::new();
        Self {
            task_store: TaskFileStore::new(),
            team_storage,
        }
    }
}

impl Default for TaskDelegateTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TaskDelegateTool {
    fn name(&self) -> &str {
        "task_delegate"
    }

    fn description(&self) -> &str {
        "Delegate a task to a specific teammate. This will assign the task to the teammate \
         and update both the task and teammate records. The task must not be blocked by \
         incomplete dependencies."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schema_for!(DelegateRequest)).unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let req: DelegateRequest = match parse_args(args) {
            Ok(r) => r,
            Err(e) => return e,
        };

        let task = match self.task_store.get_task(&req.task_id).await {
            Ok(t) => t,
            Err(e) => return err_result(format!("Task not found: {e}")),
        };

        if task.status.is_terminal() {
            return err_result(format!(
                "Cannot delegate task with status: {}",
                task.status
            ));
        }

        if !task.blocked_by.is_empty() {
            let mut blocked_by: Vec<String> = Vec::new();
            for id in &task.blocked_by {
                if let Ok(t) = self.task_store.get_task(id).await
                    && !t.status.is_terminal()
                {
                    blocked_by.push(id.clone());
                }
            }

            if !blocked_by.is_empty() {
                return err_result("Task is blocked by incomplete tasks");
            }
        }

        let teammate = match self
            .team_storage
            .teammate_store
            .get_teammate(&req.assignee)
            .await
        {
            Ok(Some(t)) => t,
            Ok(None) => {
                return err_result(format!(
                    "Teammate not found: {}",
                    req.assignee
                ));
            }
            Err(e) => {
                return err_result(format!("Failed to get teammate: {e}"));
            }
        };

        if !teammate.is_available() {
            return err_result(format!(
                "Teammate {} is not available (status: {}, current task: {:?})",
                req.assignee, teammate.status, teammate.current_task
            ));
        }

        let patch = TaskPatch::new()
            .with_owner(&req.assignee)
            .with_status(TaskStatus::InProgress);

        let updated_task =
            match self.task_store.update_task(&req.task_id, &patch).await {
                Ok(t) => t,
                Err(e) => {
                    return err_result(format!("Failed to update task: {e}"));
                }
            };

        match self
            .team_storage
            .teammate_store
            .assign_task_to_teammate(&req.assignee, &req.task_id)
            .await
        {
            Ok(_) => {}
            Err(e) => {
                let revert_patch = TaskPatch::new()
                    .with_owner("")
                    .with_status(TaskStatus::Pending);
                let _ = self
                    .task_store
                    .update_task(&req.task_id, &revert_patch)
                    .await;
                return err_result(format!(
                    "Failed to assign task to teammate: {e}"
                ));
            }
        }

        ok_result(&serde_json::json!({
            "task": updated_task,
            "assignee": req.assignee,
            "message": format!("Task {} delegated to {}", req.task_id, req.assignee)
        }))
    }
}
