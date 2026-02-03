use async_trait::async_trait;
use rmcp::model::CallToolResult;
use serde_json::Value;

use super::{
    data::TeammateStatus,
    file_store::{TeamStorage, TeammateFileStore},
    shared::err_result,
    tool_base::json_result,
    types::SpawnTeammateRequest,
};
use crate::tools::Tool;

#[derive(Clone)]
pub(crate) struct SpawnTeammateTool {
    store: TeammateFileStore,
}

impl SpawnTeammateTool {
    pub(crate) fn new() -> Self {
        Self {
            store: TeamStorage::new().teammate_store,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_storage(storage: TeamStorage) -> Self {
        Self {
            store: storage.teammate_store,
        }
    }
}

impl Default for SpawnTeammateTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for SpawnTeammateTool {
    fn name(&self) -> &str {
        "spawn_teammate"
    }

    fn description(&self) -> &str {
        "Create a new teammate agent with the specified role and initial prompt."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schemars::schema_for!(SpawnTeammateRequest))
            .unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        let request: SpawnTeammateRequest = match serde_json::from_value(args) {
            Ok(r) => r,
            Err(e) => return err_result(format!("Invalid request: {e}")),
        };

        let existing = match self.store.get_teammate(&request.name).await {
            Ok(e) => e,
            Err(e) => {
                return err_result(format!("Failed to get teammate: {e}"));
            }
        };

        if let Some(teammate) = existing
            && !matches!(
                teammate.status,
                TeammateStatus::Idle | TeammateStatus::Shutdown
            )
        {
            return err_result(format!(
                "'{}' is currently working",
                request.name
            ));
        }

        let teammate = match self
            .store
            .spawn_teammate(&request.name, &request.role)
            .await
        {
            Ok(t) => t,
            Err(e) => {
                return err_result(format!("Failed to spawn teammate: {e}"));
            }
        };

        let spawn_info = serde_json::json!({
            "teammate": teammate,
            "spawn_config": {
                "cwd": request.cwd,
                "model": request.model,
                "use_splitpane": request.use_splitpane,
                "plan_mode_required": request.plan_mode_required,
                "agent_type": request.agent_type,
            },
            "note": "Teammate record created. Actual agent spawning is handled by the host application."
        });

        json_result(&spawn_info)
    }
}

#[derive(Clone)]
pub(crate) struct ListTeammatesTool {
    store: TeammateFileStore,
}

impl ListTeammatesTool {
    pub(crate) fn new() -> Self {
        Self {
            store: TeamStorage::new().teammate_store,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_storage(storage: TeamStorage) -> Self {
        Self {
            store: storage.teammate_store,
        }
    }
}

impl Default for ListTeammatesTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ListTeammatesTool {
    fn name(&self) -> &str {
        "list_teammates"
    }

    fn description(&self) -> &str {
        "List all teammates."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn call(&self, _args: Value) -> CallToolResult {
        let teammates = match self.store.list_teammates().await {
            Ok(t) => t,
            Err(e) => {
                return err_result(format!("Failed to list teammates: {e}"));
            }
        };

        json_result(&teammates)
    }
}
