//! 7 unit tests for the `plugin_loader` module family.
//!
//! Coverage map:
//!
//! - [`super::core::AgentPluginLoader`]: 5 tests
//!   (empty_directory / loads_plugins /
//!   get_plugin / get_nonexistent_plugin /
//!   hook_runner_access).
//! - [`super::fire::AgentPluginLoader`]: 1 test
//!   (fire_events: agent_start + session_start +
//!   pre_tool_use + post_tool_use).
//! - [`super::discovery::AgentPluginLoader`]: 1 test
//!   (with_hooks: loads plugin with hooks.json).

use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::*;

fn create_test_plugin(dir: &Path, name: &str, version: &str) -> PathBuf {
    let plugin_dir = dir.join(name);
    std::fs::create_dir_all(&plugin_dir).unwrap();

    let manifest = serde_json::json!({
        "name": name,
        "version": version,
        "description": "Test plugin",
        "author": "Test"
    });

    std::fs::write(
        plugin_dir.join("plugin.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    plugin_dir
}

#[tokio::test]
async fn test_plugin_loader_empty_directory() {
    let temp = TempDir::new().unwrap();
    let loader = AgentPluginLoader::with_plugins_dir(temp.path().to_path_buf())
        .await
        .unwrap();

    assert_eq!(loader.plugin_count(), 0);
}

#[tokio::test]
async fn test_plugin_loader_loads_plugins() {
    let temp = TempDir::new().unwrap();

    // Create test plugins
    create_test_plugin(temp.path(), "test-plugin-1", "1.0.0");
    create_test_plugin(temp.path(), "test-plugin-2", "2.0.0");

    let loader = AgentPluginLoader::with_plugins_dir(temp.path().to_path_buf())
        .await
        .unwrap();

    assert_eq!(loader.plugin_count(), 2);
}

#[tokio::test]
async fn test_plugin_loader_get_plugin() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "my-test-plugin", "1.0.0");

    let loader = AgentPluginLoader::with_plugins_dir(temp.path().to_path_buf())
        .await
        .unwrap();

    let plugin = loader.get_plugin("my-test-plugin");
    assert!(plugin.is_some());
    assert_eq!(plugin.unwrap().manifest.version, "1.0.0");
}

#[tokio::test]
async fn test_plugin_loader_get_nonexistent_plugin() {
    let temp = TempDir::new().unwrap();

    let loader = AgentPluginLoader::with_plugins_dir(temp.path().to_path_buf())
        .await
        .unwrap();

    let plugin = loader.get_plugin("nonexistent");
    assert!(plugin.is_none());
}

#[tokio::test]
async fn test_plugin_loader_fire_events() {
    let temp = TempDir::new().unwrap();
    let loader = AgentPluginLoader::with_plugins_dir(temp.path().to_path_buf())
        .await
        .unwrap();

    // These should not panic even with no plugins loaded
    loader.fire_agent_start("session-123").await.unwrap();
    loader.fire_session_start("session-123").await.unwrap();
    loader.fire_pre_tool_use("read_file").await.unwrap();
    loader.fire_post_tool_use("read_file", true).await.unwrap();
}

#[tokio::test]
async fn test_plugin_loader_with_hooks() {
    let temp = TempDir::new().unwrap();

    // Create a plugin with hooks
    let plugin_dir = create_test_plugin(temp.path(), "hooks-plugin", "1.0.0");

    let hooks_json = serde_json::json!([
        {
            "event": "AgentStart",
            "handler": {"type": "Command", "value": "echo started"},
            "priority": 0
        },
        {
            "event": "SessionStart",
            "handler": {"type": "Command", "value": "echo session started"},
            "priority": 10
        }
    ]);

    std::fs::write(
        plugin_dir.join("hooks.json"),
        serde_json::to_string_pretty(&hooks_json).unwrap(),
    )
    .unwrap();

    let loader = AgentPluginLoader::with_plugins_dir(temp.path().to_path_buf())
        .await
        .unwrap();

    assert_eq!(loader.plugin_count(), 1);

    // Fire events - commands will fail but loader should handle gracefully
    loader.fire_agent_start("session-123").await.unwrap();
}

#[tokio::test]
async fn test_plugin_loader_hook_runner_access() {
    let temp = TempDir::new().unwrap();
    let loader = AgentPluginLoader::with_plugins_dir(temp.path().to_path_buf())
        .await
        .unwrap();

    let runner = loader.hook_runner();
    let runner_ref = runner.lock().await;
    assert!(runner_ref.is_empty());
}
