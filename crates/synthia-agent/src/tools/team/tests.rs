//! Tests for team tools module

use tempfile::tempdir;

use crate::tools::Tool;

fn create_test_storage(
    base: &std::path::Path,
) -> crate::tools::storage::StoragePaths {
    crate::tools::storage::StoragePaths::with_base(base.to_path_buf())
}

mod message_tools {
    use super::*;

    #[tokio::test]
    async fn test_send_message_tool_name() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool =
            crate::tools::team::message::SendMessageTool::new_with_storage(
                storage,
            );
        assert_eq!(tool.name(), "send_message");
    }

    #[tokio::test]
    async fn test_send_message_tool_description() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool =
            crate::tools::team::message::SendMessageTool::new_with_storage(
                storage,
            );
        assert!(tool.description().contains("message"));
    }

    #[tokio::test]
    async fn test_send_message_tool_parameters() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool =
            crate::tools::team::message::SendMessageTool::new_with_storage(
                storage,
            );
        let params = tool.parameters();
        assert_eq!(params["type"], "object");
        assert!(params.as_object().unwrap().contains_key("properties"));
    }

    #[tokio::test]
    async fn test_send_message_tool_call_success() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool =
            crate::tools::team::message::SendMessageTool::new_with_storage(
                storage,
            );
        let args = serde_json::json!({"to": "alice", "content": "Hello alice"});
        let result = tool.call(args).await;
        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert!(text.text.contains("alice"));
    }

    #[tokio::test]
    async fn test_send_message_tool_invalid_request() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool =
            crate::tools::team::message::SendMessageTool::new_with_storage(
                storage,
            );
        let args = serde_json::json!({"invalid": "field"});
        let result = tool.call(args).await;
        assert!(result.is_error == Some(true));
    }

    #[tokio::test]
    async fn test_read_inbox_tool_name() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool = crate::tools::team::message::ReadInboxTool::new_with_storage(
            storage,
        );
        assert_eq!(tool.name(), "read_inbox");
    }

    #[tokio::test]
    async fn test_read_inbox_tool_call_empty() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool = crate::tools::team::message::ReadInboxTool::new_with_storage(
            storage,
        );
        let result = tool.call(serde_json::json!({})).await;
        assert!(result.is_error.is_none() || result.is_error == Some(false));
    }

    #[tokio::test]
    async fn test_broadcast_tool_name() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool = crate::tools::team::message::BroadcastTool::new_with_storage(
            storage,
        );
        assert_eq!(tool.name(), "broadcast");
    }

    #[tokio::test]
    async fn test_broadcast_tool_call_no_teammates() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool = crate::tools::team::message::BroadcastTool::new_with_storage(
            storage,
        );
        let result = tool.call(serde_json::json!({"content": "Hello"})).await;
        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert!(text.text.contains("0"));
    }
}

mod protocol_tools {
    use super::*;

    #[tokio::test]
    async fn test_shutdown_request_tool_name() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool =
            crate::tools::team::protocol::ShutdownRequestTool::new_with_storage(
                storage,
            );
        assert_eq!(tool.name(), "shutdown_request");
    }

    #[tokio::test]
    async fn test_shutdown_request_tool_call_success() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool =
            crate::tools::team::protocol::ShutdownRequestTool::new_with_storage(
                storage,
            );
        let args = serde_json::json!({"teammate": "alice"});
        let result = tool.call(args).await;
        assert!(result.is_error.is_none() || result.is_error == Some(false));
    }

    #[tokio::test]
    async fn test_shutdown_request_tool_invalid_request() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool =
            crate::tools::team::protocol::ShutdownRequestTool::new_with_storage(
                storage,
            );
        let result = tool.call(serde_json::json!({})).await;
        assert!(result.is_error == Some(true));
    }

    #[tokio::test]
    async fn test_shutdown_response_tool_name() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool = crate::tools::team::protocol::ShutdownResponseTool::new_with_storage(storage);
        assert_eq!(tool.name(), "shutdown_response");
    }

    #[tokio::test]
    async fn test_shutdown_response_tool_not_found() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool = crate::tools::team::protocol::ShutdownResponseTool::new_with_storage(storage);
        let args = serde_json::json!({"request_id": "nonexistent"});
        let result = tool.call(args).await;
        assert!(result.is_error == Some(true));
    }

    #[tokio::test]
    async fn test_plan_approval_tool_name() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool =
            crate::tools::team::protocol::PlanApprovalTool::new_with_storage(
                storage,
            );
        assert_eq!(tool.name(), "plan_approval");
    }

    #[tokio::test]
    async fn test_plan_approval_tool_not_found() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool =
            crate::tools::team::protocol::PlanApprovalTool::new_with_storage(
                storage,
            );
        let args =
            serde_json::json!({"request_id": "nonexistent", "approve": true});
        let result = tool.call(args).await;
        assert!(result.is_error == Some(true));
    }

    #[tokio::test]
    async fn test_plan_approval_tool_approve() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        storage
            .protocol_store
            .create_plan_request("plan-test", "alice", "Do the task")
            .await
            .unwrap();
        let tool =
            crate::tools::team::protocol::PlanApprovalTool::new_with_storage(
                storage,
            );
        let args = serde_json::json!({"request_id": "plan-test", "approve": true, "feedback": "Looks good"});
        let result = tool.call(args).await;
        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert!(text.text.contains("approved"));
    }
}

mod team_management_tools {
    use super::*;

    #[tokio::test]
    async fn test_team_create_tool_name() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool = crate::tools::team::team_management::TeamCreateTool::new_with_storage(storage);
        assert_eq!(tool.name(), "team_create");
    }

    #[tokio::test]
    async fn test_team_create_tool_call_success() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool = crate::tools::team::team_management::TeamCreateTool::new_with_storage(storage);
        let result = tool.call(serde_json::json!({"name": "Alpha Team"})).await;
        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert!(text.text.contains("Alpha Team"));
    }

    #[tokio::test]
    async fn test_team_create_tool_empty_name() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool = crate::tools::team::team_management::TeamCreateTool::new_with_storage(storage);
        let result = tool.call(serde_json::json!({"name": "   "})).await;
        assert!(result.is_error == Some(true));
    }

    #[tokio::test]
    async fn test_team_list_tool_name() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool =
            crate::tools::team::team_management::TeamListTool::new_with_storage(
                storage,
            );
        assert_eq!(tool.name(), "team_list");
    }

    #[tokio::test]
    async fn test_team_status_tool_name() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool = crate::tools::team::team_management::TeamStatusTool::new_with_storage(storage);
        assert_eq!(tool.name(), "team_status");
    }

    #[tokio::test]
    async fn test_team_status_tool_not_found() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool = crate::tools::team::team_management::TeamStatusTool::new_with_storage(storage);
        let result = tool
            .call(serde_json::json!({"team_id": "nonexistent"}))
            .await;
        assert!(result.is_error == Some(true));
    }

    #[tokio::test]
    async fn test_team_update_tool_name() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool = crate::tools::team::team_management::TeamUpdateTool::new_with_storage(storage);
        assert_eq!(tool.name(), "team_update");
    }

    #[tokio::test]
    async fn test_team_update_tool_no_fields() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool = crate::tools::team::team_management::TeamUpdateTool::new_with_storage(storage);
        let result = tool.call(serde_json::json!({"team_id": "any-id"})).await;
        assert!(result.is_error == Some(true));
    }

    #[tokio::test]
    async fn test_team_delete_tool_name() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool = crate::tools::team::team_management::TeamDeleteTool::new_with_storage(storage);
        assert_eq!(tool.name(), "team_delete");
    }

    #[tokio::test]
    async fn test_team_delete_tool_not_found() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool = crate::tools::team::team_management::TeamDeleteTool::new_with_storage(storage);
        let result = tool
            .call(serde_json::json!({"team_id": "nonexistent"}))
            .await;
        // delete_team returns Ok even if team doesn't exist (no-op delete)
        let _ = result;
    }
}

mod teammate_tools {
    use super::*;

    #[tokio::test]
    async fn test_spawn_teammate_tool_name() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool =
            crate::tools::team::teammate::SpawnTeammateTool::new_with_storage(
                storage,
            );
        assert_eq!(tool.name(), "spawn_teammate");
    }

    #[tokio::test]
    async fn test_spawn_teammate_tool_call_success() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool =
            crate::tools::team::teammate::SpawnTeammateTool::new_with_storage(
                storage,
            );
        let args = serde_json::json!({"name": "alice", "role": "developer", "prompt": "You develop"});
        let result = tool.call(args).await;
        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert!(text.text.contains("alice"));
    }

    #[tokio::test]
    async fn test_spawn_teammate_tool_already_working() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool =
            crate::tools::team::teammate::SpawnTeammateTool::new_with_storage(
                storage,
            );
        let r1 = tool.call(serde_json::json!({"name": "carol", "role": "dev", "prompt": "You develop"})).await;
        assert!(r1.is_error.is_none() || r1.is_error == Some(false));
        let r2 = tool.call(serde_json::json!({"name": "carol", "role": "dev", "prompt": "You develop again"})).await;
        assert!(r2.is_error == Some(true));
    }

    #[tokio::test]
    async fn test_spawn_teammate_can_respawn_after_shutdown() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool =
            crate::tools::team::teammate::SpawnTeammateTool::new_with_storage(
                storage.clone(),
            );
        let r1 = tool.call(serde_json::json!({"name": "dave", "role": "dev", "prompt": "You develop"})).await;
        assert!(r1.is_error.is_none() || r1.is_error == Some(false));
        storage
            .teammate_store
            .update_teammate_status(
                "dave",
                crate::tools::team::TeammateStatus::Shutdown,
            )
            .await
            .unwrap();
        let r2 = tool.call(serde_json::json!({"name": "dave", "role": "dev", "prompt": "You develop again"})).await;
        assert!(r2.is_error.is_none() || r2.is_error == Some(false));
    }

    #[tokio::test]
    async fn test_list_teammates_tool_name() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool =
            crate::tools::team::teammate::ListTeammatesTool::new_with_storage(
                storage,
            );
        assert_eq!(tool.name(), "list_teammates");
    }

    #[tokio::test]
    async fn test_list_teammates_tool_call_empty() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let tool =
            crate::tools::team::teammate::ListTeammatesTool::new_with_storage(
                storage,
            );
        let result = tool.call(serde_json::json!({})).await;
        assert!(result.is_error.is_none() || result.is_error == Some(false));
    }

    #[tokio::test]
    async fn test_list_teammates_tool_call_with_teammates() {
        let storage = {
            let dir = tempdir().unwrap();
            let paths = create_test_storage(dir.path());
            crate::tools::team::file_store::TeamStorage::new_with_paths(paths)
        };
        let spawn_tool =
            crate::tools::team::teammate::SpawnTeammateTool::new_with_storage(
                storage.clone(),
            );
        let r1 = spawn_tool.call(serde_json::json!({"name": "frank", "role": "dev", "prompt": "You develop"})).await;
        assert!(r1.is_error.is_none() || r1.is_error == Some(false));
        let r2 = spawn_tool.call(serde_json::json!({"name": "grace", "role": "tester", "prompt": "You test"})).await;
        assert!(r2.is_error.is_none() || r2.is_error == Some(false));
        let tool =
            crate::tools::team::teammate::ListTeammatesTool::new_with_storage(
                storage,
            );
        let result = tool.call(serde_json::json!({})).await;
        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let content = &result.content[0];
        let text = content.as_text().unwrap();
        assert!(text.text.contains("frank"));
        assert!(text.text.contains("grace"));
    }
}

mod mod_tests {
    #[tokio::test]
    async fn test_register_team_tools() {
        let registry = crate::tools::ToolRegistry::new();
        crate::tools::team::register_team_tools(&registry).await;
        let tool_names = registry.tool_names();
        assert!(tool_names.contains(&"spawn_teammate".to_string()));
        assert!(tool_names.contains(&"list_teammates".to_string()));
        assert!(tool_names.contains(&"send_message".to_string()));
        assert!(tool_names.contains(&"read_inbox".to_string()));
        assert!(tool_names.contains(&"broadcast".to_string()));
        assert!(tool_names.contains(&"shutdown_request".to_string()));
        assert!(tool_names.contains(&"shutdown_response".to_string()));
        assert!(tool_names.contains(&"plan_approval".to_string()));
        assert!(tool_names.contains(&"idle".to_string()));
        assert!(tool_names.contains(&"team_create".to_string()));
        assert!(tool_names.contains(&"team_list".to_string()));
        assert!(tool_names.contains(&"team_assign".to_string()));
        assert!(tool_names.contains(&"team_status".to_string()));
        assert!(tool_names.contains(&"team_update".to_string()));
        assert!(tool_names.contains(&"team_delete".to_string()));
    }
}
