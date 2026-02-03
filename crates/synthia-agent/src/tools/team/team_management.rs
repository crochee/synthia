use async_trait::async_trait;
use rmcp::model::CallToolResult;
use schemars::schema_for;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    data::{TeamPatch, TeamStatus},
    file_store::TeamStorage,
    shared::err_result,
};
use crate::tools::{Tool, shared::ok_result};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct TeamCreateInput {
    name: String,
    lead: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct TeamCreateOutput {
    team_id: String,
    name: String,
    lead: Option<String>,
    status: String,
}

pub(super) struct TeamCreateTool {
    storage: TeamStorage,
}

impl TeamCreateTool {
    pub(super) fn new() -> Self {
        Self {
            storage: TeamStorage::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn new_with_storage(storage: TeamStorage) -> Self {
        Self { storage }
    }
}

impl Default for TeamCreateTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TeamCreateTool {
    fn name(&self) -> &str {
        "team_create"
    }

    fn description(&self) -> &str {
        "Create a new team with an optional team lead. The team can be used to organize \
         multiple agents working together on related tasks."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schema_for!(TeamCreateInput)).unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let args: TeamCreateInput = match serde_json::from_value(args) {
            Ok(a) => a,
            Err(e) => return err_result(format!("Invalid input: {e}")),
        };

        if args.name.trim().is_empty() {
            return err_result("Team name cannot be empty");
        }

        let team = match self
            .storage
            .team_store
            .create_team(&args.name, args.lead.as_deref())
            .await
        {
            Ok(t) => t,
            Err(e) => return err_result(format!("Failed to create team: {e}")),
        };

        let output = TeamCreateOutput {
            team_id: team.team_id,
            name: team.name,
            lead: team.lead,
            status: team.status.as_str().to_string(),
        };

        ok_result(&output)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct TeamListOutput {
    teams: Vec<TeamInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct TeamInfo {
    team_id: String,
    name: String,
    status: String,
    lead: Option<String>,
    task_count: usize,
}

pub(super) struct TeamListTool {
    storage: TeamStorage,
}

impl TeamListTool {
    pub(super) fn new() -> Self {
        Self {
            storage: TeamStorage::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn new_with_storage(storage: TeamStorage) -> Self {
        Self { storage }
    }
}

impl Default for TeamListTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TeamListTool {
    fn name(&self) -> &str {
        "team_list"
    }

    fn description(&self) -> &str {
        "List all teams with their status and task counts."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({})
    }

    async fn call(&self, _args: Value) -> CallToolResult {
        let teams = match self.storage.team_store.list_teams().await {
            Ok(t) => t,
            Err(e) => return err_result(format!("Failed to list teams: {e}")),
        };

        let team_infos: Vec<TeamInfo> = teams
            .into_iter()
            .map(|t| TeamInfo {
                team_id: t.team_id,
                name: t.name,
                status: t.status.as_str().to_string(),
                lead: t.lead,
                task_count: t.task_ids.len(),
            })
            .collect();

        let output = TeamListOutput { teams: team_infos };

        ok_result(&output)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct TeamAssignInput {
    team_id: String,
    task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct TeamAssignOutput {
    team_id: String,
    task_id: String,
    task_count: usize,
}

pub(super) struct TeamAssignTool {
    storage: TeamStorage,
}

impl TeamAssignTool {
    pub(super) fn new() -> Self {
        Self {
            storage: TeamStorage::new(),
        }
    }
}

impl Default for TeamAssignTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TeamAssignTool {
    fn name(&self) -> &str {
        "team_assign"
    }

    fn description(&self) -> &str {
        "Assign a task to a team. The task will be added to the team's task list."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schema_for!(TeamAssignInput)).unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let args: TeamAssignInput = match serde_json::from_value(args) {
            Ok(a) => a,
            Err(e) => return err_result(format!("Invalid input: {e}")),
        };

        let team = match self
            .storage
            .team_store
            .assign_task_to_team(&args.team_id, &args.task_id)
            .await
        {
            Ok(t) => t,
            Err(e) => {
                return err_result(format!(
                    "Failed to assign task to team: {e}"
                ));
            }
        };

        let output = TeamAssignOutput {
            team_id: team.team_id,
            task_id: args.task_id,
            task_count: team.task_ids.len(),
        };

        ok_result(&output)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct TeamStatusInput {
    team_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct TeamStatusOutput {
    team_id: String,
    name: String,
    status: String,
    lead: Option<String>,
    task_ids: Vec<String>,
    created_at: i64,
    updated_at: i64,
}

pub(super) struct TeamStatusTool {
    storage: TeamStorage,
}

impl TeamStatusTool {
    pub(super) fn new() -> Self {
        Self {
            storage: TeamStorage::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn new_with_storage(storage: TeamStorage) -> Self {
        Self { storage }
    }
}

impl Default for TeamStatusTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TeamStatusTool {
    fn name(&self) -> &str {
        "team_status"
    }

    fn description(&self) -> &str {
        "Get detailed status of a team including its tasks."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schema_for!(TeamStatusInput)).unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let args: TeamStatusInput = match serde_json::from_value(args) {
            Ok(a) => a,
            Err(e) => return err_result(format!("Invalid input: {e}")),
        };

        let team = match self.storage.team_store.get_team(&args.team_id).await {
            Ok(Some(t)) => t,
            Ok(None) => {
                return err_result(format!("Team not found: {}", args.team_id));
            }
            Err(e) => return err_result(format!("Failed to get team: {e}")),
        };

        let output = TeamStatusOutput {
            team_id: team.team_id,
            name: team.name,
            status: team.status.as_str().to_string(),
            lead: team.lead,
            task_ids: team.task_ids,
            created_at: team.created_at as i64,
            updated_at: team.updated_at as i64,
        };

        ok_result(&output)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct TeamUpdateInput {
    team_id: String,
    status: Option<String>,
    lead: Option<String>,
}

pub(super) struct TeamUpdateTool {
    storage: TeamStorage,
}

impl TeamUpdateTool {
    pub(super) fn new() -> Self {
        Self {
            storage: TeamStorage::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn new_with_storage(storage: TeamStorage) -> Self {
        Self { storage }
    }
}

impl Default for TeamUpdateTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TeamUpdateTool {
    fn name(&self) -> &str {
        "team_update"
    }

    fn description(&self) -> &str {
        "Update team status or lead. Status can be 'created', 'running', 'completed', or 'deleted'."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schema_for!(TeamUpdateInput)).unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let args: TeamUpdateInput = match serde_json::from_value(args) {
            Ok(a) => a,
            Err(e) => return err_result(format!("Invalid input: {e}")),
        };

        if args.status.is_none() && args.lead.is_none() {
            return err_result(
                "At least one of 'status' or 'lead' must be provided",
            );
        }

        let status = args.status.as_ref().and_then(|s| match s.as_str() {
            "created" => Some(TeamStatus::Created),
            "running" => Some(TeamStatus::Running),
            "completed" => Some(TeamStatus::Completed),
            "deleted" => Some(TeamStatus::Deleted),
            _ => None,
        });

        if let Some(ref invalid_status) = args.status
            && status.is_none()
        {
            return err_result(format!(
                "Invalid status: {invalid_status}. Must be one of: created, running, completed, deleted"
            ));
        }

        let patch = TeamPatch {
            status,
            lead: args.lead,
            ..Default::default()
        };

        let team = match self
            .storage
            .team_store
            .update_team(&args.team_id, &patch)
            .await
        {
            Ok(t) => t,
            Err(e) => return err_result(format!("Failed to update team: {e}")),
        };

        let output = TeamStatusOutput {
            team_id: team.team_id,
            name: team.name,
            status: team.status.as_str().to_string(),
            lead: team.lead,
            task_ids: team.task_ids,
            created_at: team.created_at as i64,
            updated_at: team.updated_at as i64,
        };

        ok_result(&output)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct TeamDeleteInput {
    team_id: String,
}

pub(super) struct TeamDeleteTool {
    storage: TeamStorage,
}

impl TeamDeleteTool {
    pub(super) fn new() -> Self {
        Self {
            storage: TeamStorage::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn new_with_storage(storage: TeamStorage) -> Self {
        Self { storage }
    }
}

impl Default for TeamDeleteTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TeamDeleteTool {
    fn name(&self) -> &str {
        "team_delete"
    }

    fn description(&self) -> &str {
        "Delete a team. This will remove the team but not its tasks."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schema_for!(TeamDeleteInput)).unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let args: TeamDeleteInput = match serde_json::from_value(args) {
            Ok(a) => a,
            Err(e) => return err_result(format!("Invalid input: {e}")),
        };

        match self.storage.team_store.delete_team(&args.team_id).await {
            Ok(_) => ok_result(&serde_json::json!({
                "message": format!("Team {} deleted", args.team_id)
            })),
            Err(e) => err_result(format!("Failed to delete team: {e}")),
        }
    }
}
