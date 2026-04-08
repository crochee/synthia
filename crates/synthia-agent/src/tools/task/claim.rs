use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;
use rmcp::model::{CallToolResult, Content};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;

use super::{
    data::{Task, TaskGraph, TaskNode, TaskPatch, TaskStatus},
    file_store::TaskFileStore,
};
use crate::tools::Tool;

/// Request for claiming a task.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClaimTaskRequest {
    /// The ID of the task to claim.
    pub task_id: String,
    /// The owner (member) who is claiming the task.
    pub owner: String,
    /// If true, check if the owner already has open tasks.
    #[serde(default)]
    pub check_busy: bool,
}

/// Request for finding and claiming an available task by team_id.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClaimAvailableTaskRequest {
    /// The team ID to match.
    pub team_id: String,
    /// The owner (member) who is claiming the task.
    pub owner: String,
    /// If true, check if the owner already has open tasks.
    #[serde(default)]
    pub check_busy: bool,
}

/// Result of a claim task operation.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClaimTaskResult {
    /// Whether the claim was successful.
    pub success: bool,
    /// The reason for failure if unsuccessful.
    pub reason: Option<ClaimTaskFailureReason>,
    /// The ID of the task that was claimed or attempted.
    pub task_id: Option<String>,
    /// IDs of tasks that are blocking this task (if blocked).
    pub blocked_by_tasks: Option<Vec<String>>,
    /// IDs of tasks the owner is already busy with (if agent busy).
    pub busy_with_tasks: Option<Vec<String>>,
}

/// Reasons why a task claim can fail.
#[derive(
    Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq,
)]
#[serde(rename_all = "snake_case")]
pub enum ClaimTaskFailureReason {
    /// The task was not found.
    TaskNotFound,
    /// The task is already claimed by another owner.
    AlreadyClaimed,
    /// The task is already resolved (completed).
    AlreadyResolved,
    /// The task is blocked by unresolved dependencies.
    Blocked,
    /// The owner already has open tasks.
    AgentBusy,
    /// No available tasks found matching the criteria.
    NoAvailableTasks,
}

/// State for managing atomic task claims.
#[derive(Debug, Default)]
struct ClaimState {
    /// Set of task IDs currently being claimed.
    pending_claims: std::collections::HashSet<String>,
}

/// Tool for claiming tasks.
#[derive(Clone)]
pub struct ClaimTaskTool {
    store: TaskFileStore,
    /// Lock for atomic claim operations.
    claim_lock: Arc<Mutex<ClaimState>>,
    /// Lock for the task graph cache.
    graph_lock: Arc<RwLock<Option<TaskGraph>>>,
}

impl ClaimTaskTool {
    /// Creates a new ClaimTaskTool instance.
    pub fn new() -> Self {
        Self {
            store: TaskFileStore::new(),
            claim_lock: Arc::new(Mutex::new(ClaimState::default())),
            graph_lock: Arc::new(RwLock::new(None)),
        }
    }

    /// Creates a ClaimTaskTool with a custom base path.
    pub fn with_base(base_path: std::path::PathBuf) -> Self {
        Self {
            store: TaskFileStore::with_base(base_path),
            claim_lock: Arc::new(Mutex::new(ClaimState::default())),
            graph_lock: Arc::new(RwLock::new(None)),
        }
    }

    /// Builds or updates the task graph from all tasks.
    async fn build_task_graph(&self) -> TaskGraph {
        let tasks = match self.store.list_tasks().await {
            Ok(t) => t,
            Err(_) => return TaskGraph::new(),
        };

        let mut graph = TaskGraph::new();
        for task in &tasks {
            graph.add_task(TaskNode::from(task));
        }

        // Add dependency edges
        for task in &tasks {
            for blocked_by in &task.blocked_by {
                graph.add_dependency(blocked_by, &task.id);
            }
        }

        graph
    }

    /// Gets or builds the task graph.
    async fn get_task_graph(&self) -> TaskGraph {
        // Try to read from cache first
        {
            let read_guard = self.graph_lock.read();
            if let Some(ref graph) = *read_guard {
                return graph.clone();
            }
        }

        // Build new graph and cache it
        let graph = self.build_task_graph().await;
        {
            let mut write_guard = self.graph_lock.write();
            *write_guard = Some(graph.clone());
        }
        graph
    }

    /// Invalidates the task graph cache.
    #[allow(dead_code)]
    fn invalidate_graph_cache(&self) {
        let mut write_guard = self.graph_lock.write();
        *write_guard = None;
    }

    /// Atomically claims a task.
    /// Returns Ok(task) on success, Err(reason) on failure.
    async fn claim_task_atomic(
        &self,
        task_id: &str,
        owner: &str,
        check_busy: bool,
    ) -> Result<Task, ClaimTaskFailureReason> {
        // Acquire the claim lock
        let mut claim_state = self.claim_lock.lock().await;

        // Check if this task is already being claimed
        if claim_state.pending_claims.contains(task_id) {
            return Err(ClaimTaskFailureReason::AlreadyClaimed);
        }

        // Mark this task as being claimed
        claim_state.pending_claims.insert(task_id.to_string());

        // Drop the lock while we do the actual work
        drop(claim_state);

        // Do the actual claim work
        let result = self.do_claim_task(task_id, owner, check_busy).await;

        // Remove from pending claims
        let mut claim_state = self.claim_lock.lock().await;
        claim_state.pending_claims.remove(task_id);

        result
    }

    /// Performs the actual task claim logic.
    async fn do_claim_task(
        &self,
        task_id: &str,
        owner: &str,
        check_busy: bool,
    ) -> Result<Task, ClaimTaskFailureReason> {
        // Get the task
        let task = self
            .store
            .get_task(task_id)
            .await
            .map_err(|_| ClaimTaskFailureReason::TaskNotFound)?;

        // Check if already claimed by someone else
        if !task.owner.is_empty() && task.owner != owner {
            return Err(ClaimTaskFailureReason::AlreadyClaimed);
        }

        // Check if already resolved
        if task.status == TaskStatus::Completed {
            return Err(ClaimTaskFailureReason::AlreadyResolved);
        }

        // Check dependencies using TaskGraph
        let graph = self.get_task_graph().await;
        if !graph.check_dependencies(task_id) {
            return Err(ClaimTaskFailureReason::Blocked);
        }

        // Check if owner is busy with other tasks
        if check_busy {
            let all_tasks = self
                .store
                .list_tasks()
                .await
                .map_err(|_| ClaimTaskFailureReason::TaskNotFound)?;

            if all_tasks.iter().any(|t| {
                t.status != TaskStatus::Completed
                    && !t.owner.is_empty()
                    && t.owner == owner
                    && t.id != task_id
            }) {
                return Err(ClaimTaskFailureReason::AgentBusy);
            }
        }

        // Update the task
        let patch = TaskPatch::new()
            .with_owner(owner)
            .with_status(TaskStatus::InProgress);

        self.store
            .update_task(task_id, &patch)
            .await
            .map_err(|_| ClaimTaskFailureReason::TaskNotFound)
    }

    /// Finds and claims an available task for a team.
    pub async fn claim_available_task(
        &self,
        team_id: &str,
        owner: &str,
        check_busy: bool,
    ) -> Result<Task, ClaimTaskFailureReason> {
        // Get all tasks
        let all_tasks = self
            .store
            .list_tasks()
            .await
            .map_err(|_| ClaimTaskFailureReason::NoAvailableTasks)?;

        // Check if owner is busy with other tasks
        if check_busy
            && all_tasks.iter().any(|t| {
                t.status != TaskStatus::Completed
                    && !t.owner.is_empty()
                    && t.owner == owner
            })
        {
            return Err(ClaimTaskFailureReason::AgentBusy);
        }

        // Get task graph for dependency checking
        let graph = self.get_task_graph().await;

        // Find pending tasks matching team_id with satisfied dependencies
        let available_tasks: Vec<&Task> = all_tasks
            .iter()
            .filter(|t| {
                t.status == TaskStatus::Pending
                    && t.owner.is_empty()
                    && t.team_id.as_deref() == Some(team_id)
                    && graph.check_dependencies(&t.id)
            })
            .collect();

        if available_tasks.is_empty() {
            return Err(ClaimTaskFailureReason::NoAvailableTasks);
        }

        // Sort by priority (highest first)
        let mut available_tasks = available_tasks;
        available_tasks
            .sort_by(|a, b| b.priority.level().cmp(&a.priority.level()));

        // Try to claim the highest priority task
        for task in available_tasks {
            if let Ok(claimed) =
                self.claim_task_atomic(&task.id, owner, false).await
            {
                return Ok(claimed);
            }
        }

        Err(ClaimTaskFailureReason::NoAvailableTasks)
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
        "Claim a task for the specified owner. Checks dependencies using TaskGraph \
         and supports atomic claiming to prevent race conditions."
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

        match self
            .claim_task_atomic(
                &request.task_id,
                &request.owner,
                request.check_busy,
            )
            .await
        {
            Ok(task) => build_result(true, None, &task.id, None, None),
            Err(ClaimTaskFailureReason::Blocked) => {
                // Get blocking tasks for detailed error
                let task = self.store.get_task(&request.task_id).await;
                let blocked_by = task.map(|t| t.blocked_by).unwrap_or_default();
                build_result(
                    false,
                    Some(ClaimTaskFailureReason::Blocked),
                    &request.task_id,
                    Some(blocked_by),
                    None,
                )
            }
            Err(reason) => {
                build_result(false, Some(reason), &request.task_id, None, None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::tools::task::data::TaskPriority;

    /// Helper to create a test task.
    fn create_test_task(
        id: &str,
        subject: &str,
        status: TaskStatus,
        owner: &str,
        team_id: Option<&str>,
    ) -> Task {
        let mut task =
            Task::new(id, subject).with_status(status).with_owner(owner);
        if let Some(tid) = team_id {
            task = task.with_team(tid);
        }
        task
    }

    #[tokio::test]
    async fn test_claim_task_success() {
        let dir = tempdir().unwrap();
        let tool = ClaimTaskTool::with_base(dir.path().to_path_buf());

        // Create a pending task
        let task = create_test_task(
            "task-1",
            "Test Task",
            TaskStatus::Pending,
            "",
            None,
        );
        tool.store.create_task(&task).await.unwrap();

        // Claim the task
        let result = tool
            .claim_task_atomic("task-1", "agent-1", false)
            .await
            .unwrap();

        assert_eq!(result.id, "task-1");
        assert_eq!(result.owner, "agent-1");
        assert_eq!(result.status, TaskStatus::InProgress);

        // Verify the task was updated in storage
        let stored = tool.store.get_task("task-1").await.unwrap();
        assert_eq!(stored.owner, "agent-1");
        assert_eq!(stored.status, TaskStatus::InProgress);
    }

    #[tokio::test]
    async fn test_claim_task_not_found() {
        let dir = tempdir().unwrap();
        let tool = ClaimTaskTool::with_base(dir.path().to_path_buf());

        let result = tool
            .claim_task_atomic("nonexistent", "agent-1", false)
            .await;

        assert!(matches!(result, Err(ClaimTaskFailureReason::TaskNotFound)));
    }

    #[tokio::test]
    async fn test_claim_task_already_claimed() {
        let dir = tempdir().unwrap();
        let tool = ClaimTaskTool::with_base(dir.path().to_path_buf());

        // Create a task already claimed by another agent
        let task = create_test_task(
            "task-1",
            "Test Task",
            TaskStatus::InProgress,
            "agent-1",
            None,
        );
        tool.store.create_task(&task).await.unwrap();

        // Try to claim with a different agent
        let result = tool.claim_task_atomic("task-1", "agent-2", false).await;

        assert!(matches!(
            result,
            Err(ClaimTaskFailureReason::AlreadyClaimed)
        ));
    }

    #[tokio::test]
    async fn test_claim_task_already_resolved() {
        let dir = tempdir().unwrap();
        let tool = ClaimTaskTool::with_base(dir.path().to_path_buf());

        // Create a completed task
        let task = create_test_task(
            "task-1",
            "Test Task",
            TaskStatus::Completed,
            "",
            None,
        );
        tool.store.create_task(&task).await.unwrap();

        let result = tool.claim_task_atomic("task-1", "agent-1", false).await;

        assert!(matches!(
            result,
            Err(ClaimTaskFailureReason::AlreadyResolved)
        ));
    }

    #[tokio::test]
    async fn test_claim_task_blocked_by_dependencies() {
        let dir = tempdir().unwrap();
        let tool = ClaimTaskTool::with_base(dir.path().to_path_buf());

        // Create a dependency task (pending)
        let dep_task = create_test_task(
            "dep-1",
            "Dependency",
            TaskStatus::Pending,
            "",
            None,
        );
        tool.store.create_task(&dep_task).await.unwrap();

        // Create a task that depends on dep-1
        let mut task = create_test_task(
            "task-1",
            "Test Task",
            TaskStatus::Pending,
            "",
            None,
        );
        task.blocked_by = vec!["dep-1".to_string()];
        tool.store.create_task(&task).await.unwrap();

        // Try to claim the blocked task
        let result = tool.claim_task_atomic("task-1", "agent-1", false).await;

        assert!(matches!(result, Err(ClaimTaskFailureReason::Blocked)));
    }

    #[tokio::test]
    async fn test_claim_task_dependencies_satisfied() {
        let dir = tempdir().unwrap();
        let tool = ClaimTaskTool::with_base(dir.path().to_path_buf());

        // Create a completed dependency task
        let dep_task = create_test_task(
            "dep-1",
            "Dependency",
            TaskStatus::Completed,
            "agent-0",
            None,
        );
        tool.store.create_task(&dep_task).await.unwrap();

        // Create a task that depends on dep-1
        let mut task = create_test_task(
            "task-1",
            "Test Task",
            TaskStatus::Pending,
            "",
            None,
        );
        task.blocked_by = vec!["dep-1".to_string()];
        tool.store.create_task(&task).await.unwrap();

        // Claim the task (should succeed since dependency is completed)
        let result = tool
            .claim_task_atomic("task-1", "agent-1", false)
            .await
            .unwrap();

        assert_eq!(result.id, "task-1");
        assert_eq!(result.owner, "agent-1");
        assert_eq!(result.status, TaskStatus::InProgress);
    }

    #[tokio::test]
    async fn test_claim_task_agent_busy() {
        let dir = tempdir().unwrap();
        let tool = ClaimTaskTool::with_base(dir.path().to_path_buf());

        // Create a task already owned by agent-1
        let busy_task = create_test_task(
            "busy-1",
            "Busy Task",
            TaskStatus::InProgress,
            "agent-1",
            None,
        );
        tool.store.create_task(&busy_task).await.unwrap();

        // Create another task to claim
        let task = create_test_task(
            "task-1",
            "Test Task",
            TaskStatus::Pending,
            "",
            None,
        );
        tool.store.create_task(&task).await.unwrap();

        // Try to claim with check_busy=true
        let result = tool.claim_task_atomic("task-1", "agent-1", true).await;

        assert!(matches!(result, Err(ClaimTaskFailureReason::AgentBusy)));
    }

    #[tokio::test]
    async fn test_claim_available_task_by_team() {
        let dir = tempdir().unwrap();
        let tool = ClaimTaskTool::with_base(dir.path().to_path_buf());

        // Create tasks for team-1
        let task1 = create_test_task(
            "task-1",
            "Task 1",
            TaskStatus::Pending,
            "",
            Some("team-1"),
        );
        let task2 = create_test_task(
            "task-2",
            "Task 2",
            TaskStatus::Pending,
            "",
            Some("team-1"),
        );
        let task3 = create_test_task(
            "task-3",
            "Task 3",
            TaskStatus::Pending,
            "",
            Some("team-2"),
        );

        tool.store.create_task(&task1).await.unwrap();
        tool.store.create_task(&task2).await.unwrap();
        tool.store.create_task(&task3).await.unwrap();

        // Claim an available task for team-1
        let result = tool
            .claim_available_task("team-1", "agent-1", false)
            .await
            .unwrap();

        assert!(result.id == "task-1" || result.id == "task-2");
        assert_eq!(result.owner, "agent-1");
        assert_eq!(result.status, TaskStatus::InProgress);
    }

    #[tokio::test]
    async fn test_claim_available_task_no_tasks() {
        let dir = tempdir().unwrap();
        let tool = ClaimTaskTool::with_base(dir.path().to_path_buf());

        // Create a task for a different team
        let task = create_test_task(
            "task-1",
            "Task 1",
            TaskStatus::Pending,
            "",
            Some("team-2"),
        );
        tool.store.create_task(&task).await.unwrap();

        // Try to claim for team-1
        let result =
            tool.claim_available_task("team-1", "agent-1", false).await;

        assert!(matches!(
            result,
            Err(ClaimTaskFailureReason::NoAvailableTasks)
        ));
    }

    #[tokio::test]
    async fn test_claim_available_task_priority_ordering() {
        let dir = tempdir().unwrap();
        let tool = ClaimTaskTool::with_base(dir.path().to_path_buf());

        // Create tasks with different priorities
        let task_low = Task::new("task-low", "Low Priority")
            .with_status(TaskStatus::Pending)
            .with_team("team-1")
            .with_priority(TaskPriority::Low);

        let task_critical = Task::new("task-critical", "Critical Priority")
            .with_status(TaskStatus::Pending)
            .with_team("team-1")
            .with_priority(TaskPriority::Critical);

        let task_normal = Task::new("task-normal", "Normal Priority")
            .with_status(TaskStatus::Pending)
            .with_team("team-1")
            .with_priority(TaskPriority::Normal);

        tool.store.create_task(&task_low).await.unwrap();
        tool.store.create_task(&task_critical).await.unwrap();
        tool.store.create_task(&task_normal).await.unwrap();

        // Claim should get the critical priority task
        let result = tool
            .claim_available_task("team-1", "agent-1", false)
            .await
            .unwrap();

        assert_eq!(result.id, "task-critical");
        assert_eq!(result.priority, TaskPriority::Critical);
    }

    #[tokio::test]
    async fn test_atomic_claim_prevents_race_condition() {
        let dir = tempdir().unwrap();
        let tool = ClaimTaskTool::with_base(dir.path().to_path_buf());
        let tool_clone = tool.clone();
        let store = tool.store.clone();

        // Create a pending task
        let task = create_test_task(
            "task-1",
            "Test Task",
            TaskStatus::Pending,
            "",
            None,
        );
        tool.store.create_task(&task).await.unwrap();

        // Spawn two concurrent claim attempts
        let handle1 = tokio::spawn(async move {
            tool.claim_task_atomic("task-1", "agent-1", false).await
        });

        let handle2 = tokio::spawn(async move {
            tool_clone
                .claim_task_atomic("task-1", "agent-2", false)
                .await
        });

        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        // Exactly one should succeed
        let success_count = [result1.is_ok(), result2.is_ok()]
            .iter()
            .filter(|&&x| x)
            .count();
        assert_eq!(success_count, 1);

        // Verify the final owner
        let stored = store.get_task("task-1").await.unwrap();
        assert!(stored.owner == "agent-1" || stored.owner == "agent-2");
        assert_eq!(stored.status, TaskStatus::InProgress);
    }

    #[tokio::test]
    async fn test_claim_task_same_owner_reclaim() {
        let dir = tempdir().unwrap();
        let tool = ClaimTaskTool::with_base(dir.path().to_path_buf());

        // Create a task already claimed by agent-1
        let task = create_test_task(
            "task-1",
            "Test Task",
            TaskStatus::InProgress,
            "agent-1",
            None,
        );
        tool.store.create_task(&task).await.unwrap();

        // Same owner can reclaim (idempotent)
        let result = tool
            .claim_task_atomic("task-1", "agent-1", false)
            .await
            .unwrap();

        assert_eq!(result.id, "task-1");
        assert_eq!(result.owner, "agent-1");
    }

    #[tokio::test]
    async fn test_task_graph_check_dependencies() {
        let dir = tempdir().unwrap();
        let tool = ClaimTaskTool::with_base(dir.path().to_path_buf());

        // Create a chain of tasks: task-1 -> task-2 -> task-3
        let task1 = create_test_task(
            "task-1",
            "Task 1",
            TaskStatus::Completed,
            "agent-0",
            None,
        );
        let mut task2 =
            create_test_task("task-2", "Task 2", TaskStatus::Pending, "", None);
        task2.blocked_by = vec!["task-1".to_string()];
        let mut task3 =
            create_test_task("task-3", "Task 3", TaskStatus::Pending, "", None);
        task3.blocked_by = vec!["task-2".to_string()];

        tool.store.create_task(&task1).await.unwrap();
        tool.store.create_task(&task2).await.unwrap();
        tool.store.create_task(&task3).await.unwrap();

        // task-2 should be claimable (dependency completed)
        let result2 = tool
            .claim_task_atomic("task-2", "agent-1", false)
            .await
            .unwrap();
        assert_eq!(result2.id, "task-2");

        // task-3 should be blocked (task-2 not completed yet)
        let result3 = tool.claim_task_atomic("task-3", "agent-1", false).await;
        assert!(matches!(result3, Err(ClaimTaskFailureReason::Blocked)));
    }

    #[tokio::test]
    async fn test_tool_call_success() {
        let dir = tempdir().unwrap();
        let tool = ClaimTaskTool::with_base(dir.path().to_path_buf());

        // Create a pending task
        let task = create_test_task(
            "task-1",
            "Test Task",
            TaskStatus::Pending,
            "",
            None,
        );
        tool.store.create_task(&task).await.unwrap();

        // Call the tool
        let args = serde_json::json!({
            "task_id": "task-1",
            "owner": "agent-1"
        });

        let result = tool.call(args).await;
        assert_eq!(result.is_error, Some(false));

        // Parse the result
        let content = &result.content[0];
        if let rmcp::model::RawContent::Text(text) = &content.raw {
            let claim_result: ClaimTaskResult =
                serde_json::from_str(&text.text).unwrap();
            assert!(claim_result.success);
            assert_eq!(claim_result.task_id, Some("task-1".to_string()));
        }
    }

    #[tokio::test]
    async fn test_tool_call_blocked() {
        let dir = tempdir().unwrap();
        let tool = ClaimTaskTool::with_base(dir.path().to_path_buf());

        // Create a dependency task (pending)
        let dep_task = create_test_task(
            "dep-1",
            "Dependency",
            TaskStatus::Pending,
            "",
            None,
        );
        tool.store.create_task(&dep_task).await.unwrap();

        // Create a blocked task
        let mut task = create_test_task(
            "task-1",
            "Test Task",
            TaskStatus::Pending,
            "",
            None,
        );
        task.blocked_by = vec!["dep-1".to_string()];
        tool.store.create_task(&task).await.unwrap();

        // Call the tool
        let args = serde_json::json!({
            "task_id": "task-1",
            "owner": "agent-1"
        });

        let result = tool.call(args).await;
        assert_eq!(result.is_error, Some(false));

        // Parse the result
        let content = &result.content[0];
        if let rmcp::model::RawContent::Text(text) = &content.raw {
            let claim_result: ClaimTaskResult =
                serde_json::from_str(&text.text).unwrap();
            assert!(!claim_result.success);
            assert_eq!(
                claim_result.reason,
                Some(ClaimTaskFailureReason::Blocked)
            );
            assert!(claim_result.blocked_by_tasks.is_some());
        }
    }
}
