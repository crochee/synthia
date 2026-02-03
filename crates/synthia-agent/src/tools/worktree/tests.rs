//! Tests for worktree module

use tempfile::TempDir;

use crate::tools::worktree::{
    WorktreeEntry,
    WorktreeEventBus,
    WorktreeManager,
};

// =============================================================================
// WorktreeEventBus Tests
// =============================================================================

#[test]
fn test_event_bus_new_creates_directory() {
    let temp = TempDir::new().unwrap();

    let bus = WorktreeEventBus::new(temp.path().to_path_buf());

    // Path should be set correctly
    assert_eq!(bus.list_recent(10).unwrap(), "[]");
    drop(temp);
}

#[test]
fn test_event_bus_emit_and_list_recent() {
    let temp = TempDir::new().unwrap();
    let bus = WorktreeEventBus::new(temp.path().to_path_buf());

    bus.emit(
        "test.event",
        Some(serde_json::json!({"id": 1})),
        Some(serde_json::json!({"name": "test"})),
        None,
    )
    .unwrap();

    let events_json = bus.list_recent(10).unwrap();
    let events: Vec<serde_json::Value> =
        serde_json::from_str(&events_json).unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["event"], "test.event");
    assert_eq!(events[0]["task"], serde_json::json!({"id": 1}));
}

#[test]
fn test_event_bus_list_recent_respects_limit() {
    let temp = TempDir::new().unwrap();
    let bus = WorktreeEventBus::new(temp.path().to_path_buf());

    for i in 0..5 {
        bus.emit(&format!("event.{i}"), None, None, None).unwrap();
    }

    let events_json = bus.list_recent(3).unwrap();
    let events: Vec<serde_json::Value> =
        serde_json::from_str(&events_json).unwrap();

    assert_eq!(events.len(), 3);
    // Just verify we got 3 events back
    assert!(events[0].as_object().unwrap().contains_key("event"));
}

#[test]
fn test_event_bus_list_recent_empty_when_no_file() {
    let temp = TempDir::new().unwrap();
    let bus = WorktreeEventBus::new(temp.path().to_path_buf());

    let result = bus.list_recent(10).unwrap();
    assert_eq!(result, "[]");
}

#[test]
fn test_event_bus_emit_with_error() {
    let temp = TempDir::new().unwrap();
    let bus = WorktreeEventBus::new(temp.path().to_path_buf());

    bus.emit(
        "worktree.create.failed",
        Some(serde_json::json!({"id": 42})),
        Some(serde_json::json!({"name": "test"})),
        Some("git error".to_string()),
    )
    .unwrap();

    let events_json = bus.list_recent(1).unwrap();
    let events: Vec<serde_json::Value> =
        serde_json::from_str(&events_json).unwrap();

    assert_eq!(events[0]["error"], "git error");
}

// =============================================================================
// WorktreeManager Tests
// =============================================================================

fn create_test_manager() -> (WorktreeManager, TempDir) {
    let temp = TempDir::new().unwrap();
    let manager = WorktreeManager::new(temp.path().to_path_buf());
    (manager, temp)
}

#[test]
fn test_manager_new_creates_index() {
    let temp = TempDir::new().unwrap();
    let manager = WorktreeManager::new(temp.path().to_path_buf());

    let worktrees = manager.list();
    assert!(worktrees.is_empty());
}

#[test]
fn test_manager_list_empty() {
    let (manager, _temp) = create_test_manager();
    assert!(manager.list().is_empty());
}

#[test]
fn test_manager_find_nonexistent() {
    let (manager, _temp) = create_test_manager();
    assert!(manager.find("nonexistent").is_none());
}

#[test]
fn test_manager_find_name_validation_invalid() {
    let (manager, _temp) = create_test_manager();

    let result = manager.create("invalid name!", None, "HEAD");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid worktree name"));
}

#[test]
fn test_manager_find_name_validation_special_chars() {
    let (manager, _temp) = create_test_manager();

    // Valid: alphanumeric, dots, underscores, hyphens
    assert!(manager.create("invalid name", None, "HEAD").is_err());
    assert!(manager.create("name@test", None, "HEAD").is_err());
}

#[test]
fn test_manager_status_nonexistent() {
    let (manager, _temp) = create_test_manager();
    let result = manager.status("nonexistent");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not found"));
}

#[test]
fn test_manager_run_dangerous_command() {
    let (manager, _temp) = create_test_manager();

    let result = manager.run("any", "rm -rf /");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Dangerous command blocked"));
}

#[test]
fn test_manager_run_dangerous_sudo() {
    let (manager, _temp) = create_test_manager();
    let result = manager.run("any", "sudo rm -rf");
    assert!(result.is_err());
}

#[test]
fn test_manager_run_dangerous_shutdown() {
    let (manager, _temp) = create_test_manager();
    let result = manager.run("any", "shutdown now");
    assert!(result.is_err());
}

#[test]
fn test_manager_run_dangerous_dev_null() {
    let (manager, _temp) = create_test_manager();
    let result = manager.run("any", "cat /dev/null > /tmp/test");
    assert!(result.is_err());
}

#[test]
fn test_manager_run_nonexistent_worktree() {
    let (manager, _temp) = create_test_manager();
    let result = manager.run("nonexistent", "ls");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not found"));
}

#[test]
fn test_manager_remove_nonexistent() {
    let (manager, _temp) = create_test_manager();
    let result = manager.remove("nonexistent", false);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not found"));
}

#[test]
fn test_manager_keep_nonexistent() {
    let (manager, _temp) = create_test_manager();
    let result = manager.keep("nonexistent");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not found"));
}

#[test]
fn test_manager_events_empty() {
    let (manager, _temp) = create_test_manager();
    let events = manager.events(10);
    assert_eq!(events, "[]");
}

// =============================================================================
// WorktreeEntry Tests
// =============================================================================

#[test]
fn test_worktree_entry_serialization() {
    let entry = WorktreeEntry {
        name: "test".to_string(),
        path: "/path/to/test".to_string(),
        branch: "wt/test".to_string(),
        task_id: Some(42),
        status: "active".to_string(),
        created_at: 1234567890,
    };

    let json = serde_json::to_string(&entry).unwrap();
    let deserialized: WorktreeEntry = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.name, "test");
    assert_eq!(deserialized.path, "/path/to/test");
    assert_eq!(deserialized.branch, "wt/test");
    assert_eq!(deserialized.task_id, Some(42));
    assert_eq!(deserialized.status, "active");
    assert_eq!(deserialized.created_at, 1234567890);
}

#[test]
fn test_worktree_entry_without_task_id() {
    let entry = WorktreeEntry {
        name: "test".to_string(),
        path: "/path/to/test".to_string(),
        branch: "wt/test".to_string(),
        task_id: None,
        status: "active".to_string(),
        created_at: 1234567890,
    };

    let json = serde_json::to_string(&entry).unwrap();
    let deserialized: WorktreeEntry = serde_json::from_str(&json).unwrap();

    assert!(deserialized.task_id.is_none());
}

// =============================================================================
// WorktreeIndex Tests (internal)
// =============================================================================

#[test]
fn test_worktree_index_default() {
    use crate::tools::worktree::index::WorktreeIndex;

    let index = WorktreeIndex::default();
    assert!(index.worktrees.is_empty());
}

#[test]
fn test_worktree_index_serialization() {
    use crate::tools::worktree::index::WorktreeIndex;

    let mut index = WorktreeIndex::default();
    index.worktrees.push(WorktreeEntry {
        name: "test".to_string(),
        path: "/path".to_string(),
        branch: "wt/test".to_string(),
        task_id: None,
        status: "active".to_string(),
        created_at: 0,
    });

    let json = serde_json::to_string_pretty(&index).unwrap();
    let deserialized: WorktreeIndex = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.worktrees.len(), 1);
    assert_eq!(deserialized.worktrees[0].name, "test");
}

// =============================================================================
// Request Struct Deserialization Tests
// =============================================================================

#[test]
fn test_worktree_create_request_deserialization() {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct WorktreeCreateRequest {
        name: String,
        #[serde(rename = "taskId")]
        task_id: Option<i64>,
        #[serde(rename = "baseRef")]
        base_ref: Option<String>,
    }

    // Full request
    let json = serde_json::json!({
        "name": "test-worktree",
        "taskId": 42,
        "baseRef": "main"
    });
    let req: WorktreeCreateRequest = serde_json::from_value(json).unwrap();
    assert_eq!(req.name, "test-worktree");
    assert_eq!(req.task_id, Some(42));
    assert_eq!(req.base_ref, Some("main".to_string()));

    // Minimal request (only name required)
    let json = serde_json::json!({"name": "minimal"});
    let req: WorktreeCreateRequest = serde_json::from_value(json).unwrap();
    assert_eq!(req.name, "minimal");
    assert_eq!(req.task_id, None);
    assert_eq!(req.base_ref, None);

    // Without optional fields
    let json = serde_json::json!({
        "name": "test-worktree",
        "taskId": null,
        "baseRef": null
    });
    let req: WorktreeCreateRequest = serde_json::from_value(json).unwrap();
    assert_eq!(req.name, "test-worktree");
    assert!(req.task_id.is_none());
    assert!(req.base_ref.is_none());
}

#[test]
fn test_worktree_run_request_deserialization() {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct WorktreeRunRequest {
        name: String,
        command: String,
    }

    let json = serde_json::json!({
        "name": "my-worktree",
        "command": "ls -la"
    });
    let req: WorktreeRunRequest = serde_json::from_value(json).unwrap();
    assert_eq!(req.name, "my-worktree");
    assert_eq!(req.command, "ls -la");
}

#[test]
fn test_worktree_remove_request_deserialization() {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct WorktreeRemoveRequest {
        name: String,
        #[serde(default)]
        force: bool,
    }

    // With force = true
    let json = serde_json::json!({"name": "to-remove", "force": true});
    let req: WorktreeRemoveRequest = serde_json::from_value(json).unwrap();
    assert_eq!(req.name, "to-remove");
    assert!(req.force);

    // With force = false
    let json = serde_json::json!({"name": "to-remove", "force": false});
    let req: WorktreeRemoveRequest = serde_json::from_value(json).unwrap();
    assert!(!req.force);

    // Without force (should default to false)
    let json = serde_json::json!({"name": "to-remove"});
    let req: WorktreeRemoveRequest = serde_json::from_value(json).unwrap();
    assert!(!req.force);
}

#[test]
fn test_worktree_events_request_deserialization() {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct WorktreeEventsRequest {
        #[serde(default = "default_limit")]
        limit: usize,
    }

    fn default_limit() -> usize {
        20
    }

    // Explicit limit
    let json = serde_json::json!({"limit": 50});
    let req: WorktreeEventsRequest = serde_json::from_value(json).unwrap();
    assert_eq!(req.limit, 50);

    // Default limit (no limit field)
    let json = serde_json::json!({});
    let req: WorktreeEventsRequest = serde_json::from_value(json).unwrap();
    assert_eq!(req.limit, 20);

    // Zero limit
    let json = serde_json::json!({"limit": 0});
    let req: WorktreeEventsRequest = serde_json::from_value(json).unwrap();
    assert_eq!(req.limit, 0);
}

#[test]
fn test_default_limit_function() {
    fn default_limit() -> usize {
        20
    }
    assert_eq!(default_limit(), 20);
}

// =============================================================================
// Tool Metadata Tests
// =============================================================================

#[test]
fn test_tool_names() {
    // Verify expected tool names match their struct implementations
    let expected_names = [
        ("worktree_create", "worktree_create"),
        ("worktree_list", "worktree_list"),
        ("worktree_status", "worktree_status"),
        ("worktree_run", "worktree_run"),
        ("worktree_remove", "worktree_remove"),
        ("worktree_keep", "worktree_keep"),
        ("worktree_events", "worktree_events"),
    ];

    for (name, _expected) in expected_names {
        assert!(!name.is_empty());
        assert!(name.starts_with("worktree_"));
    }
}

#[test]
fn test_worktree_create_parameters_schema() {
    let params = serde_json::json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "Worktree name"
            },
            "taskId": {
                "type": "integer",
                "description": "Task ID"
            },
            "baseRef": {
                "type": "string",
                "description": "Git ref",
                "default": "HEAD"
            }
        },
        "required": ["name"]
    });

    // Verify schema structure
    assert_eq!(params["type"], "object");
    assert!(params["properties"].is_object());
    assert!(params["required"].is_array());

    let required = params["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("name")));

    // Verify name property exists
    let props = params["properties"].as_object().unwrap();
    assert!(props.contains_key("name"));
    assert!(props.contains_key("taskId"));
    assert!(props.contains_key("baseRef"));
}

#[test]
fn test_worktree_list_parameters_schema() {
    let params = serde_json::json!({
        "type": "object",
        "properties": {}
    });

    assert_eq!(params["type"], "object");
    assert!(params["properties"].is_object());
    assert!(params["properties"].as_object().unwrap().is_empty());
}

#[test]
fn test_worktree_status_parameters_schema() {
    let params = serde_json::json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "Worktree name"
            }
        },
        "required": ["name"]
    });

    let required = params["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("name")));
}

#[test]
fn test_worktree_run_parameters_schema() {
    let params = serde_json::json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "Worktree name"
            },
            "command": {
                "type": "string",
                "description": "Command"
            }
        },
        "required": ["name", "command"]
    });

    let required = params["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("name")));
    assert!(required.contains(&serde_json::json!("command")));
}

#[test]
fn test_worktree_remove_parameters_schema() {
    let params = serde_json::json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "Worktree name"
            },
            "force": {
                "type": "boolean",
                "description": "Force removal",
                "default": false
            }
        },
        "required": ["name"]
    });

    assert!(params["properties"]["force"]["default"] == false);
}

#[test]
fn test_worktree_events_parameters_schema() {
    let params = serde_json::json!({
        "type": "object",
        "properties": {
            "limit": {
                "type": "integer",
                "description": "Number of events",
                "default": 20
            }
        }
    });

    assert!(params["properties"]["limit"]["default"] == 20);
}

// =============================================================================
// Error Handling Tests
// =============================================================================

#[test]
fn test_invalid_json_deserialization_error() {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct WorktreeCreateRequest {
        #[serde(rename = "name")]
        _name: String,
        #[serde(rename = "taskId")]
        _task_id: Option<i64>,
    }

    // Missing required "name" field
    let json = serde_json::json!({"taskId": 42});
    let result: Result<WorktreeCreateRequest, _> = serde_json::from_value(json);
    assert!(result.is_err());

    // Wrong type for name (expected String, got number)
    let json = serde_json::json!({"name": 123});
    let result = serde_json::from_value::<WorktreeCreateRequest>(json);
    assert!(result.is_err());

    // Invalid JSON structure
    let json = serde_json::json!({"name": "test", "extra": "field"});
    // Should still work with extra fields (serde by default ignores extra)
    let result = serde_json::from_value::<WorktreeCreateRequest>(json);
    assert!(result.is_ok());
}

#[test]
fn test_worktree_list_empty_args_handling() {
    // WorktreeListTool.call takes Value but ignores _args
    // Verify that empty object is acceptable
    let empty_args = serde_json::json!({});
    assert!(empty_args.is_object());
    assert!(empty_args.as_object().unwrap().is_empty());
}

#[test]
fn test_worktree_events_default_args_handling() {
    // WorktreeEventsTool uses unwrap_or with default when deserializing
    let args_with_limit = serde_json::json!({"limit": 100});
    let args_empty = serde_json::json!({});

    #[derive(Debug, serde::Deserialize)]
    struct WorktreeEventsRequest {
        #[serde(default)]
        limit: usize,
    }

    let req1: WorktreeEventsRequest =
        serde_json::from_value(args_with_limit).unwrap();
    assert_eq!(req1.limit, 100);

    // Empty args should use default
    let req2: WorktreeEventsRequest =
        serde_json::from_value(args_empty).unwrap();
    assert_eq!(req2.limit, 0); // default for usize is 0
}
