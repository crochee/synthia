use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use synthia_provider::{
    Content,
    ContentPart,
    types::{Message, Role},
};

use super::*;
use crate::types::{AgentConfig, TokenUsage};

fn make_checkpoint(session_id: &str, dir: &Path, step: usize) -> Checkpoint {
    Checkpoint::new(session_id.to_string(), dir.to_path_buf())
        .with_step(step)
        .with_iteration(step)
        .with_token_usage(TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
        })
}

#[test]
fn test_checkpoint_save_and_load() {
    let dir = tempfile::tempdir().unwrap();
    let cp = make_checkpoint("sess-1", dir.path(), 1);
    cp.save().unwrap();

    let loaded = Checkpoint::load_latest(dir.path()).unwrap();
    assert!(loaded.is_some());
    let data = loaded.unwrap();
    assert_eq!(data.session_id, "sess-1");
    assert_eq!(data.step, 1);
    assert_eq!(data.iteration, 1);
    assert_eq!(data.token_usage.total_tokens, 150);
}

#[test]
fn test_checkpoint_rotate_keeps_max_5() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..7 {
        let cp = make_checkpoint("sess-1", dir.path(), i);
        cp.save().unwrap();
    }

    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(entries.len(), MAX_CHECKPOINTS);

    // Verify the oldest steps (0, 1) were removed, latest (2-6) remain
    let mut steps: Vec<_> = entries
        .iter()
        .filter_map(|e| {
            e.path()
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|stem| stem.strip_prefix("step_"))
                .and_then(|n| n.parse::<usize>().ok())
        })
        .collect();
    steps.sort();
    assert_eq!(steps, vec![2, 3, 4, 5, 6]);
}

#[test]
fn test_checkpoint_load_latest_empty_dir() {
    let dir = tempfile::tempdir().unwrap();
    let result = Checkpoint::load_latest(dir.path()).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_checkpoint_load_latest_nonexistent_dir() {
    let dir = tempfile::tempdir().unwrap();
    let nonexistent = dir.path().join("does_not_exist");
    let result = Checkpoint::load_latest(&nonexistent).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_checkpoint_load_by_session() {
    let dir = tempfile::tempdir().unwrap();
    let session_dir = dir.path().join("my-session");
    let cp = make_checkpoint("my-session", &session_dir, 3);
    cp.save().unwrap();

    let loaded =
        Checkpoint::load_latest_by_session(dir.path(), "my-session").unwrap();
    assert!(loaded.is_some());
    assert_eq!(loaded.unwrap().session_id, "my-session");

    // Wrong session returns None
    let loaded =
        Checkpoint::load_latest_by_session(dir.path(), "wrong-session")
            .unwrap();
    assert!(loaded.is_none());
}

#[test]
fn test_guardian_state_serialization() {
    let state = GuardianState {
        loop_counts: HashMap::from([("toolA".to_string(), 3)]),
        no_progress: true,
        consecutive_errors: 2,
        circuit_breaker_open: true,
    };

    let json = serde_json::to_string(&state).unwrap();
    let restored: GuardianState = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.consecutive_errors, 2);
    assert!(restored.circuit_breaker_open);
    assert!(restored.no_progress);
    assert_eq!(restored.loop_counts.get("toolA"), Some(&3));
}

#[test]
fn test_guardian_state_default() {
    let state = GuardianState::default();
    assert!(!state.circuit_breaker_open);
    assert_eq!(state.consecutive_errors, 0);
    assert!(!state.no_progress);
    assert!(state.loop_counts.is_empty());
}

#[test]
fn test_agent_config_snapshot_from_config() {
    let config = AgentConfig {
        model: "test-model".to_string(),
        max_tokens: 2048,
        max_iterations: 50,
        temperature: Some(0.7),
        workspace_root: PathBuf::from("/tmp"),
        token_budget: Some(100_000),
        checkpoint_dir: None,
        context_token_budget: None,
        observability: None,
        compaction_provider: None,
        ..Default::default()
    };

    let snapshot = AgentConfigSnapshot::from_config(&config);
    assert_eq!(snapshot.model, "test-model");
    assert_eq!(snapshot.max_tokens, 2048);
    assert_eq!(snapshot.max_iterations, 50);
    assert_eq!(snapshot.temperature, Some(0.7));
    assert_eq!(snapshot.token_budget, Some(100_000));
}

#[test]
fn test_agent_config_snapshot_serialization() {
    let snapshot = AgentConfigSnapshot::default();
    let json = serde_json::to_string(&snapshot).unwrap();
    let restored: AgentConfigSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.model, snapshot.model);
}

#[test]
fn test_pending_tool_call_serialization() {
    let call = PendingToolCall {
        id: "call-1".to_string(),
        name: "read_file".to_string(),
        input: serde_json::json!({"path": "/tmp/test.txt"}),
    };

    let json = serde_json::to_string(&call).unwrap();
    let restored: PendingToolCall = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.id, "call-1");
    assert_eq!(restored.name, "read_file");
}

#[test]
fn test_checkpoint_data_full_serialization() {
    let data = CheckpointData {
        session_id: "s1".to_string(),
        step: 5,
        timestamp: chrono::Utc::now().to_rfc3339(),
        messages: vec![Message::user("hello")],
        pending_tool_calls: vec![PendingToolCall {
            id: "call-1".to_string(),
            name: "test".to_string(),
            input: serde_json::json!({}),
        }],
        agent_config: AgentConfigSnapshot::default(),
        guardian_state: GuardianState::default(),
        token_usage: TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
        },
        iteration: 5,
    };

    let json = serde_json::to_string(&data).unwrap();
    let restored: CheckpointData = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.session_id, "s1");
    assert_eq!(restored.step, 5);
    assert_eq!(restored.iteration, 5);
    assert_eq!(restored.messages.len(), 1);
    assert_eq!(restored.pending_tool_calls.len(), 1);
}

#[test]
fn test_checkpoint_builder_pattern() {
    let dir = tempfile::tempdir().unwrap();
    let cp = Checkpoint::new("s1".to_string(), dir.path().to_path_buf())
        .with_step(10)
        .with_iteration(10)
        .with_token_usage(TokenUsage {
            prompt_tokens: 500,
            completion_tokens: 250,
            total_tokens: 750,
            cached_prompt_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
        })
        .with_guardian_state(GuardianState {
            loop_counts: HashMap::new(),
            no_progress: false,
            consecutive_errors: 1,
            circuit_breaker_open: false,
        });

    cp.save().unwrap();

    let loaded = Checkpoint::load_latest(dir.path()).unwrap().unwrap();
    assert_eq!(loaded.step, 10);
    assert_eq!(loaded.iteration, 10);
    assert_eq!(loaded.token_usage.total_tokens, 750);
    assert_eq!(loaded.guardian_state.consecutive_errors, 1);
}

#[test]
fn test_patch_tool_calls_recovery() {
    let tool_use = synthia_provider::types::ToolUse {
        id: "call-1".to_string(),
        name: "test_tool".to_string(),
        input: serde_json::json!({"path": "/tmp"}),
    };
    let mut msgs = vec![Message {
        role: Role::Assistant,
        content: Content::Single(ContentPart::ToolUse(tool_use)),
        tool_call_id: None,
        name: None,
        ..Default::default()
    }];

    let patched = patch_tool_calls_recovery(&mut msgs);
    assert_eq!(patched, 1);

    if let Content::Single(ContentPart::ToolUse(tu)) = &msgs[0].content {
        assert_eq!(tu.input.get("status").unwrap(), "executing");
    } else {
        panic!("Expected ToolUse content");
    }
}

#[test]
fn test_patch_tool_calls_recovery_already_has_status() {
    let tool_use = synthia_provider::types::ToolUse {
        id: "call-1".to_string(),
        name: "test_tool".to_string(),
        input: serde_json::json!({"path": "/tmp", "status": "done"}),
    };
    let mut msgs = vec![Message {
        role: Role::Assistant,
        content: Content::Single(ContentPart::ToolUse(tool_use)),
        tool_call_id: None,
        name: None,
        ..Default::default()
    }];

    let patched = patch_tool_calls_recovery(&mut msgs);
    assert_eq!(patched, 0);
}

#[test]
fn test_patch_tool_calls_recovery_no_tool_calls() {
    let mut msgs = vec![Message::user("hello")];
    let patched = patch_tool_calls_recovery(&mut msgs);
    assert_eq!(patched, 0);
}

#[test]
fn test_patch_tool_calls_recovery_non_object_input() {
    let tool_use = synthia_provider::types::ToolUse {
        id: "call-1".to_string(),
        name: "test_tool".to_string(),
        input: serde_json::json!("simple string"),
    };
    let mut msgs = vec![Message {
        role: Role::Assistant,
        content: Content::Single(ContentPart::ToolUse(tool_use)),
        tool_call_id: None,
        name: None,
        ..Default::default()
    }];

    let patched = patch_tool_calls_recovery(&mut msgs);
    assert_eq!(patched, 1);

    if let Content::Single(ContentPart::ToolUse(tu)) = &msgs[0].content {
        assert!(tu.input.is_object());
        assert_eq!(tu.input.get("status").unwrap(), "executing");
    }
}

#[test]
fn test_rotate_standalone_function() {
    let dir = tempfile::tempdir().unwrap();

    // Manually create checkpoint files
    for i in 0..8 {
        let path = dir.path().join(format!("step_{}.json", i));
        let data = CheckpointData {
            session_id: "s1".to_string(),
            step: i,
            timestamp: chrono::Utc::now().to_rfc3339(),
            messages: vec![],
            pending_tool_calls: vec![],
            agent_config: AgentConfigSnapshot::default(),
            guardian_state: GuardianState::default(),
            token_usage: TokenUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                cached_prompt_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
            },
            iteration: i,
        };
        let json = serde_json::to_string(&data).unwrap();
        std::fs::write(path, json).unwrap();
    }

    Checkpoint::rotate(dir.path(), 5).unwrap();

    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(entries.len(), 5);

    let mut steps: Vec<_> = entries
        .iter()
        .filter_map(|e| {
            e.path()
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|stem| stem.strip_prefix("step_"))
                .and_then(|n| n.parse::<usize>().ok())
        })
        .collect();
    steps.sort();
    assert_eq!(steps, vec![3, 4, 5, 6, 7]);
}
