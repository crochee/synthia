use async_trait::async_trait;
use rmcp::model::CallToolResult;
use schemars::schema_for;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    data::{Task, TaskPacket, TaskPriority, TaskStatus},
    file_store::TaskFileStore,
    shared::{err_result, ok_result, parse_args},
};
use crate::tools::Tool;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct CreateRequest {
    subject: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    status: Option<TaskStatus>,
    #[serde(default)]
    blocked_by: Option<Vec<String>>,
    #[serde(default)]
    blocks: Option<Vec<String>>,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    team_id: Option<String>,
    #[serde(default)]
    priority: Option<TaskPriority>,
    #[serde(default)]
    deadline: Option<i64>,
    #[serde(default)]
    task_packet: Option<TaskPacketInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct TaskPacketInput {
    #[serde(default)]
    objective: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    branch_policy: Option<String>,
    #[serde(default)]
    acceptance_tests: Option<Vec<String>>,
    #[serde(default)]
    commit_policy: Option<String>,
    #[serde(default)]
    reporting_contract: Option<String>,
    #[serde(default)]
    escalation_policy: Option<String>,
}

impl From<TaskPacketInput> for TaskPacket {
    fn from(input: TaskPacketInput) -> Self {
        TaskPacket {
            objective: input.objective.unwrap_or_default(),
            scope: input.scope.unwrap_or_default(),
            repo: input.repo.unwrap_or_default(),
            branch_policy: input.branch_policy.unwrap_or_default(),
            acceptance_tests: input.acceptance_tests.unwrap_or_default(),
            commit_policy: input.commit_policy.unwrap_or_default(),
            reporting_contract: input.reporting_contract.unwrap_or_default(),
            escalation_policy: input.escalation_policy.unwrap_or_default(),
        }
    }
}

#[derive(Clone)]
pub struct TaskCreateTool {
    store: TaskFileStore,
}

impl TaskCreateTool {
    pub fn new() -> Self {
        Self {
            store: TaskFileStore::new(),
        }
    }
}

impl Default for TaskCreateTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TaskCreateTool {
    fn name(&self) -> &str {
        "task_create"
    }

    fn description(&self) -> &str {
        "Create a new task with optional dependencies, priority, team assignment, and task packet."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schema_for!(CreateRequest)).unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let req: CreateRequest = match parse_args(args) {
            Ok(r) => r,
            Err(e) => return e,
        };

        let now = chrono::Utc::now().timestamp();
        let task = Task {
            id: uuid::Uuid::new_v4().to_string(),
            subject: req.subject,
            description: req.description.unwrap_or_default(),
            status: req.status.unwrap_or_default(),
            blocked_by: req.blocked_by.unwrap_or_default(),
            blocks: req.blocks.unwrap_or_default(),
            owner: req.owner.unwrap_or_default(),
            team_id: req.team_id,
            priority: req.priority.unwrap_or_default(),
            task_packet: req.task_packet.map(TaskPacket::from),
            deadline: req.deadline,
            created_at: now,
            updated_at: now,
            output: String::new(),
            messages: Vec::new(),
        };

        match self.store.create_task(&task).await {
            Ok(_) => ok_result(&task),
            Err(e) => err_result(format!("failed to create task: {e}")),
        }
    }
}
