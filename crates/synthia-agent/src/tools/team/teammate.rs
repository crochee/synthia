use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use serde_json::Value;

use super::{
    data::TeammateStatus,
    file_store::{TeamStorage, TeammateFileStore},
    shared::err_result,
    tool_base::json_result,
    types::{MemberConfig, SpawnTeammateRequest},
};
use crate::{config::AgentName, tools::Tool};

#[derive(Clone)]
pub(crate) struct SpawnTeammateTool {
    store: TeammateFileStore,
    parent_name: AgentName,
}

impl SpawnTeammateTool {
    pub(crate) fn new() -> Self {
        Self {
            store: TeamStorage::new().teammate_store,
            parent_name: AgentName::Solo,
        }
    }

    pub(crate) fn with_parent_name(mut self, name: AgentName) -> Self {
        self.parent_name = name;
        self
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn new_with_storage(storage: TeamStorage) -> Self {
        Self {
            store: storage.teammate_store,
            parent_name: AgentName::Solo,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_storage_and_name(
        storage: TeamStorage,
        name: AgentName,
    ) -> Self {
        Self {
            store: storage.teammate_store,
            parent_name: name,
        }
    }

    fn check_name(&self) -> Result<(), String> {
        if !self.parent_name.is_lead() {
            return Err(
                "This tool is only available for Team Lead.".to_string()
            );
        }
        Ok(())
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
        "Create a new teammate agent with the specified role and initial prompt. \
         Only available for Team Lead. The teammate will be spawned internally \
         and configured with the provided settings."
    }

    fn parameters(&self) -> Value {
        serde_json::to_value(schemars::schema_for!(SpawnTeammateRequest))
            .unwrap_or_default()
    }

    async fn call(&self, args: Value) -> CallToolResult {
        if let Err(e) = self.check_name() {
            return CallToolResult::error(vec![Content::text(e)]);
        }

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

        let member_config = MemberConfig::from_request(&request, None);

        let spawn_info = serde_json::json!({
            "teammate": teammate,
            "member_config": member_config,
            "spawn_status": {
                "internal": true,
                "ready": true,
                "message": "Teammate spawned successfully and ready for task assignment."
            }
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
        "List all teammates. Available in Team mode (Lead or Member)."
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

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::tools::storage::StoragePaths;

    fn create_test_storage(base: &std::path::Path) -> StoragePaths {
        StoragePaths::with_base(base.to_path_buf())
    }

    #[test]
    fn test_spawn_teammate_tool_name() {
        let tool = SpawnTeammateTool::new();
        assert_eq!(tool.name(), "spawn_teammate");
    }

    #[test]
    fn test_spawn_teammate_tool_description() {
        let tool = SpawnTeammateTool::new();
        assert!(tool.description().contains("Team Lead"));
    }

    #[tokio::test]
    async fn test_spawn_teammate_tool_solo_mode_blocked() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let storage = TeamStorage::new_with_paths(paths);
        let tool = SpawnTeammateTool::new_with_storage_and_name(
            storage,
            AgentName::Solo,
        );
        let args = serde_json::json!({"name": "alice", "role": "developer", "prompt": "You develop"});
        let result = tool.call(args).await;
        assert!(result.is_error == Some(true));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert!(text.text.contains("Team Lead"));
    }

    #[tokio::test]
    async fn test_spawn_teammate_tool_custom_mode_blocked() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let storage = TeamStorage::new_with_paths(paths);
        let tool = SpawnTeammateTool::new_with_storage_and_name(
            storage,
            AgentName::Custom("member".to_string()),
        );
        let args = serde_json::json!({"name": "alice", "role": "developer", "prompt": "You develop"});
        let result = tool.call(args).await;
        assert!(result.is_error == Some(true));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert!(text.text.contains("Team Lead"));
    }

    #[tokio::test]
    async fn test_spawn_teammate_tool_lead_mode_success() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let storage = TeamStorage::new_with_paths(paths.clone());
        let tool = SpawnTeammateTool::new_with_storage_and_name(
            storage,
            AgentName::Lead,
        );
        let args = serde_json::json!({"name": "alice", "role": "developer", "prompt": "You develop"});
        let result = tool.call(args).await;
        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert!(text.text.contains("alice"));
        assert!(text.text.contains("member_config"));
    }

    #[tokio::test]
    async fn test_spawn_teammate_tool_already_working() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let storage = TeamStorage::new_with_paths(paths.clone());
        let tool = SpawnTeammateTool::new_with_storage_and_name(
            storage.clone(),
            AgentName::Lead,
        );
        let r1 = tool.call(serde_json::json!({"name": "carol", "role": "dev", "prompt": "You develop"})).await;
        assert!(r1.is_error.is_none() || r1.is_error == Some(false));
        let r2 = tool.call(serde_json::json!({"name": "carol", "role": "dev", "prompt": "You develop again"})).await;
        assert!(r2.is_error == Some(true));
    }

    #[tokio::test]
    async fn test_spawn_teammate_can_respawn_after_shutdown() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let storage = TeamStorage::new_with_paths(paths.clone());
        let tool = SpawnTeammateTool::new_with_storage_and_name(
            storage.clone(),
            AgentName::Lead,
        );
        let r1 = tool.call(serde_json::json!({"name": "dave", "role": "dev", "prompt": "You develop"})).await;
        assert!(r1.is_error.is_none() || r1.is_error == Some(false));
        storage
            .teammate_store
            .update_teammate_status("dave", TeammateStatus::Shutdown)
            .await
            .unwrap();
        let r2 = tool.call(serde_json::json!({"name": "dave", "role": "dev", "prompt": "You develop again"})).await;
        assert!(r2.is_error.is_none() || r2.is_error == Some(false));
    }

    #[test]
    fn test_list_teammates_tool_name() {
        let tool = ListTeammatesTool::new();
        assert_eq!(tool.name(), "list_teammates");
    }

    #[tokio::test]
    async fn test_list_teammates_tool_call_empty() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let storage = TeamStorage::new_with_paths(paths);
        let tool = ListTeammatesTool::new_with_storage(storage);
        let result = tool.call(serde_json::json!({})).await;
        assert!(result.is_error.is_none() || result.is_error == Some(false));
    }
}
