use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    data::{TaskPatch, TaskStatus},
    file_store::TaskFileStore,
};
use crate::tools::Tool;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClaimTaskRequest {
    pub task_id: String,
    pub owner: String,
    #[serde(default)]
    pub check_busy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClaimTaskResult {
    pub success: bool,
    pub reason: Option<ClaimTaskFailureReason>,
    pub task_id: Option<String>,
    pub blocked_by_tasks: Option<Vec<String>>,
    pub busy_with_tasks: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClaimTaskFailureReason {
    TaskNotFound,
    AlreadyClaimed,
    AlreadyResolved,
    Blocked,
    AgentBusy,
}

#[derive(Clone)]
pub struct ClaimTaskTool {
    store: TaskFileStore,
}

impl ClaimTaskTool {
    pub fn new() -> Self {
        Self {
            store: TaskFileStore::new(),
        }
    }
}

impl Default for ClaimTaskTool {
    fn default() -> Self {
        Self::new()
    }
}

fn build_result(
    success: bool,
    reason: Option<ClaimTaskFailureReason>,
    task_id: &str,
    blocked_by_tasks: Option<Vec<String>>,
    busy_with_tasks: Option<Vec<String>>,
) -> CallToolResult {
    let result = ClaimTaskResult {
        success,
        reason,
        task_id: Some(task_id.to_string()),
        blocked_by_tasks,
        busy_with_tasks,
    };
    CallToolResult::success(vec![Content::text(
        serde_json::to_string(&result).unwrap_or_default(),
    )])
}

#[async_trait]
impl Tool for ClaimTaskTool {
    fn name(&self) -> &str {
        "claim_task"
    }

    fn description(&self) -> &str {
        "Claim a task for the specified owner. Optionally checks if agent already has open tasks."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schemars::schema_for!(ClaimTaskRequest))
            .unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: ClaimTaskRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Invalid request: {e}"
                ))]);
            }
        };

        if request.owner.is_empty() {
            return CallToolResult::error(vec![Content::text(
                "owner cannot be empty",
            )]);
        }

        let task = match self.store.get_task(&request.task_id).await {
            Ok(t) => t,
            Err(_) => {
                return build_result(
                    false,
                    Some(ClaimTaskFailureReason::TaskNotFound),
                    &request.task_id,
                    None,
                    None,
                );
            }
        };

        if !task.owner.is_empty() && task.owner != request.owner {
            return build_result(
                false,
                Some(ClaimTaskFailureReason::AlreadyClaimed),
                &request.task_id,
                None,
                None,
            );
        }

        if task.status == TaskStatus::Completed {
            return build_result(
                false,
                Some(ClaimTaskFailureReason::AlreadyResolved),
                &request.task_id,
                None,
                None,
            );
        }

        let all_tasks = match self.store.list_tasks().await {
            Ok(tasks) => tasks,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to list tasks: {e}"
                ))]);
            }
        };

        let unresolved_ids: std::collections::HashSet<String> = all_tasks
            .iter()
            .filter(|t| t.status != TaskStatus::Completed)
            .map(|t| t.id.clone())
            .collect();

        let blocked_by_unresolved: Vec<String> = task
            .blocked_by
            .iter()
            .filter(|id| unresolved_ids.contains(id.as_str()))
            .cloned()
            .collect();

        if !blocked_by_unresolved.is_empty() {
            return build_result(
                false,
                Some(ClaimTaskFailureReason::Blocked),
                &request.task_id,
                Some(blocked_by_unresolved),
                None,
            );
        }

        if request.check_busy {
            let busy_tasks: Vec<String> = all_tasks
                .iter()
                .filter(|t| {
                    t.status != TaskStatus::Completed
                        && !t.owner.is_empty()
                        && t.owner == request.owner
                        && t.id != request.task_id
                })
                .map(|t| t.id.clone())
                .collect();

            if !busy_tasks.is_empty() {
                return build_result(
                    false,
                    Some(ClaimTaskFailureReason::AgentBusy),
                    &request.task_id,
                    None,
                    Some(busy_tasks),
                );
            }
        }

        match self
            .store
            .update_task(
                &request.task_id,
                &TaskPatch {
                    owner: Some(request.owner.clone()),
                    status: Some(TaskStatus::InProgress),
                    ..Default::default()
                },
            )
            .await
        {
            Ok(updated) => build_result(true, None, &updated.id, None, None),
            Err(e) => CallToolResult::error(vec![Content::text(format!(
                "Failed to claim task: {e}"
            ))]),
        }
    }
}
