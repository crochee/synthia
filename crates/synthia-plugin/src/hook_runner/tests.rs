//! Unit tests for the `hook_runner` module family.
//!
//! Coverage map (9 tests):
//!
//! - Loading: 3 tests
//!   (array-form hooks.json, regex matcher compiles, invalid regex
//!   yields `HookRunnerError::InvalidRegex`).
//! - [`super::fire::fire`]: 4 tests
//!   (single event fires, wrong event yields no results, regex
//!   matcher filters targets, priority ordering).
//! - [`super::types::HookMetadata`]: 1 test
//!   (builder pattern: new + 2× with_extra + target_str).
//! - [`SharedHookRunner`]: 1 test
//!   (mutex lock, is_empty).

use std::{path::PathBuf, sync::Arc};

use tempfile::TempDir;
use tokio::sync::Mutex;

use super::{types::HookRunnerError, *};
use crate::types::HookEvent;

// =============================================================================
// Test Helpers
// =============================================================================

fn create_hooks_json(hooks_json: &str) -> (PathBuf, TempDir) {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("hooks.json");
    std::fs::write(&path, hooks_json).unwrap();
    (path, temp)
}

// =============================================================================
// Loading Tests
// =============================================================================

#[test]
fn test_load_hooks_json_array() {
    let json = r#"[
        {
            "event": "AgentStart",
            "handler": {"type": "Command", "value": "echo started"},
            "priority": 10
        },
        {
            "event": "PreToolUse",
            "handler": {"type": "Prompt", "value": "Check tool"},
            "priority": 5
        }
    ]"#;

    let (path, _temp) = create_hooks_json(json);
    let mut runner = HookRunner::new();
    runner.load_from_file(&path).unwrap();

    assert_eq!(runner.len(), 2);
    // PreToolUse (priority 5) should come before AgentStart (priority 10)
    assert_eq!(runner.configs()[0].event, HookEvent::PreToolUse);
    assert_eq!(runner.configs()[1].event, HookEvent::AgentStart);
}

#[test]
fn test_load_hooks_with_matcher() {
    let json = r#"[
        {
            "event": "PreToolUse",
            "matcher": "^read_.*",
            "handler": {"type": "Command", "value": "check.sh"},
            "priority": 0
        }
    ]"#;

    let (path, _temp) = create_hooks_json(json);
    let mut runner = HookRunner::new();
    runner.load_from_file(&path).unwrap();

    assert_eq!(runner.len(), 1);
    assert!(runner.matchers[0].is_some());
}

#[test]
fn test_invalid_regex() {
    let json = r#"[
        {
            "event": "AgentStart",
            "matcher": "[invalid",
            "handler": {"type": "Command", "value": "echo test"}
        }
    ]"#;

    let (path, _temp) = create_hooks_json(json);
    let mut runner = HookRunner::new();
    let err = runner.load_from_file(&path).unwrap_err();
    assert!(matches!(err, HookRunnerError::InvalidRegex(_, _)));
}

// =============================================================================
// fire Tests
// =============================================================================

#[tokio::test]
async fn test_fire_single_event() {
    let json = r#"[
        {
            "event": "AgentStart",
            "handler": {"type": "Command", "value": "echo hello"},
            "priority": 0
        }
    ]"#;

    let (path, _temp) = create_hooks_json(json);
    let mut runner = HookRunner::new();
    runner.load_from_file(&path).unwrap();

    let metadata = HookMetadata::new("test-agent");
    let results = runner.fire(HookEvent::AgentStart, metadata).await.unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].config.event, HookEvent::AgentStart);
}

#[tokio::test]
async fn test_fire_wrong_event_no_matches() {
    let json = r#"[
        {
            "event": "AgentStart",
            "handler": {"type": "Command", "value": "echo hello"},
            "priority": 0
        }
    ]"#;

    let (path, _temp) = create_hooks_json(json);
    let mut runner = HookRunner::new();
    runner.load_from_file(&path).unwrap();

    // Fire different event
    let metadata = HookMetadata::new("test-tool");
    let results = runner.fire(HookEvent::PreToolUse, metadata).await.unwrap();

    assert!(results.is_empty());
}

#[tokio::test]
async fn test_fire_with_matcher_filter() {
    let json = r#"[
        {
            "event": "PreToolUse",
            "matcher": "^read_.*",
            "handler": {"type": "Command", "value": "echo matched"},
            "priority": 0
        }
    ]"#;

    let (path, _temp) = create_hooks_json(json);
    let mut runner = HookRunner::new();
    runner.load_from_file(&path).unwrap();

    // Should match
    let metadata = HookMetadata::new("read_file");
    let results = runner.fire(HookEvent::PreToolUse, metadata).await.unwrap();
    assert_eq!(results.len(), 1);

    // Should not match
    let metadata = HookMetadata::new("write_file");
    let results = runner.fire(HookEvent::PreToolUse, metadata).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_priority_ordering() {
    let json = r#"[
        {
            "event": "AgentStart",
            "handler": {"type": "Command", "value": "echo third"},
            "priority": 30
        },
        {
            "event": "AgentStart",
            "handler": {"type": "Command", "value": "echo first"},
            "priority": 10
        },
        {
            "event": "AgentStart",
            "handler": {"type": "Command", "value": "echo second"},
            "priority": 20
        }
    ]"#;

    let (path, _temp) = create_hooks_json(json);
    let mut runner = HookRunner::new();
    runner.load_from_file(&path).unwrap();

    assert_eq!(runner.len(), 3);
    assert_eq!(runner.configs()[0].priority, 10);
    assert_eq!(runner.configs()[1].priority, 20);
    assert_eq!(runner.configs()[2].priority, 30);
}

// =============================================================================
// HookMetadata Test
// =============================================================================

#[test]
fn test_hook_metadata() {
    let meta = HookMetadata::new("my-tool")
        .with_extra("env", "prod")
        .with_extra("region", "us-east");

    assert_eq!(meta.target_str(), "my-tool");
    assert_eq!(meta.extras.get("env"), Some(&"prod".to_string()));
    assert_eq!(meta.extras.get("region"), Some(&"us-east".to_string()));
}

// =============================================================================
// SharedHookRunner Test
// =============================================================================

#[test]
fn test_shared_hook_runner() {
    let runner = HookRunner::new();
    let shared: SharedHookRunner = Arc::new(Mutex::new(runner));

    let runner_clone = shared.clone();
    let runner_ref = runner_clone.blocking_lock();
    assert!(runner_ref.is_empty());
}
