//! Unit tests for the `registry` module family.
//!
//! Coverage map (13 tests):
//!
//! - Discovery: 2 tests
//!   ([`PluginRegistry::discover_plugins_in_dir`] on a non-existent
//!   dir returns empty, on a populated dir returns only entries
//!   that contain a `plugin.json`).
//! - Lifecycle: 4 tests
//!   (load + unload happy path, unload of an unknown id fails,
//!   duplicate name detection, `clear` empties the registry).
//! - Hook loading: 1 test
//!   (parses hooks.json with both string-form and
//!   object-form entries, picking up `timeout`).
//! - MCP loading: 1 test
//!   (parses mcp.json with command / args / env).
//! - Manifest errors: 1 test
//!   (missing `plugin.json` → [`PluginError::ManifestNotFound`]).
//! - Accessors: 2 tests
//!   ([`PluginRegistry::get_by_name`] finds / misses,
//!   [`PluginRegistry::clear`] resets state).
//! - Helpers: 1 test function
//!   (`create_test_plugin` writes a minimal plugin dir for use by
//!   the other tests; not a test itself, so 12 actual tests).

use std::{fs, path::Path};

use tempfile::TempDir;
use uuid::Uuid;

use super::*;
use crate::PluginError;

// =============================================================================
// Test Helpers
// =============================================================================

fn create_test_plugin(dir: &Path, name: &str, version: &str) -> PluginPath {
    let plugin_dir = dir.join(name);
    fs::create_dir_all(&plugin_dir).unwrap();

    let manifest = serde_json::json!({
        "name": name,
        "version": version,
        "description": "Test plugin",
        "author": "Test"
    });

    fs::write(
        plugin_dir.join("plugin.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    PluginPath::new(plugin_dir)
}

// =============================================================================
// Discovery Tests
// =============================================================================

#[test]
fn test_discover_user_plugins_nonexistent_dir() {
    // Use a temp directory that doesn't exist in the plugin search path
    let temp = TempDir::new().unwrap();
    // Point to a non-existent directory
    let result = PluginRegistry::discover_plugins_in_dir(
        &temp.path().join("nonexistent"),
    );
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_discover_plugins_in_directory() {
    let temp = TempDir::new().unwrap();
    let plugins_dir = temp.path();

    // Create two plugins
    let plugin1_path = create_test_plugin(plugins_dir, "plugin-one", "1.0.0");
    let plugin2_path = create_test_plugin(plugins_dir, "plugin-two", "2.0.0");

    // Create a non-plugin directory (no plugin.json)
    let non_plugin = plugins_dir.join("not-a-plugin");
    fs::create_dir_all(&non_plugin).unwrap();

    let discovered =
        PluginRegistry::discover_plugins_in_dir(plugins_dir).unwrap();
    assert_eq!(discovered.len(), 2);

    let paths: Vec<_> = discovered
        .iter()
        .map(|p| p.as_path().to_path_buf())
        .collect();
    assert!(paths.contains(&plugin1_path.as_path().to_path_buf()));
    assert!(paths.contains(&plugin2_path.as_path().to_path_buf()));
    assert!(!paths.contains(&non_plugin));
}

// =============================================================================
// Lifecycle Tests
// =============================================================================

#[test]
fn test_load_and_unload_plugin() {
    let temp = TempDir::new().unwrap();
    let plugin_path = create_test_plugin(temp.path(), "test-plugin", "1.0.0");

    let mut registry = PluginRegistry::new();
    let handle = registry.load_plugin(&plugin_path).unwrap();

    assert_eq!(registry.len(), 1);
    assert_eq!(handle.manifest.name, "test-plugin");
    assert_eq!(handle.manifest.version, "1.0.0");

    // Unload should succeed
    let result = registry.unload_plugin(&handle.id);
    assert!(result.is_ok());
    assert!(registry.is_empty());
}

#[test]
fn test_unload_nonexistent_plugin() {
    let mut registry = PluginRegistry::new();
    let fake_id = Uuid::new_v4();
    let result = registry.unload_plugin(&fake_id);
    assert!(result.is_err());
}

#[test]
fn test_duplicate_plugin_name() {
    let temp = TempDir::new().unwrap();
    let plugin_path =
        create_test_plugin(temp.path(), "duplicate-test", "1.0.0");

    let mut registry = PluginRegistry::new();
    registry.load_plugin(&plugin_path).unwrap();

    // Try to load the same plugin again
    let result = registry.load_plugin(&plugin_path);
    assert!(matches!(result, Err(PluginError::DuplicatePlugin(_))));
}

#[test]
fn test_registry_clear() {
    let temp = TempDir::new().unwrap();
    let plugin_path =
        create_test_plugin(temp.path(), "clearable-plugin", "1.0.0");

    let mut registry = PluginRegistry::new();
    registry.load_plugin(&plugin_path).unwrap();
    assert!(!registry.is_empty());

    registry.clear();
    assert!(registry.is_empty());
}

// =============================================================================
// Hook Loading Test
// =============================================================================

#[test]
fn test_load_with_hooks() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("hooks-plugin");
    fs::create_dir_all(&plugin_dir).unwrap();

    let manifest = serde_json::json!({
        "name": "hooks-plugin",
        "version": "1.0.0",
        "description": "Plugin with hooks",
        "author": "Test"
    });
    fs::write(
        plugin_dir.join("plugin.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let hooks_json = serde_json::json!({
        "pre-task": "./scripts/pre-task.sh",
        "post-task": {
            "path": "./scripts/post-task.sh",
            "timeout": 30
        }
    });
    fs::write(
        plugin_dir.join("hooks.json"),
        serde_json::to_string_pretty(&hooks_json).unwrap(),
    )
    .unwrap();

    let path = PluginPath::new(plugin_dir);
    let handle = PluginHandle::load(&path).unwrap();

    assert_eq!(handle.hooks.len(), 2);

    // Find hooks by name (order is not guaranteed due to HashMap)
    let pre_task = handle.hooks.iter().find(|h| h.name == "pre-task").unwrap();
    assert_eq!(pre_task.path, "./scripts/pre-task.sh");
    assert_eq!(pre_task.timeout_seconds, None);

    let post_task =
        handle.hooks.iter().find(|h| h.name == "post-task").unwrap();
    assert_eq!(post_task.path, "./scripts/post-task.sh");
    assert_eq!(post_task.timeout_seconds, Some(30));
}

// =============================================================================
// MCP Loading Test
// =============================================================================

#[test]
fn test_load_with_mcp_config() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("mcp-plugin");
    fs::create_dir_all(&plugin_dir).unwrap();

    let manifest = serde_json::json!({
        "name": "mcp-plugin",
        "version": "1.0.0",
        "description": "Plugin with MCP servers",
        "author": "Test"
    });
    fs::write(
        plugin_dir.join("plugin.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let mcp_json = serde_json::json!({
        "example-server": {
            "command": "node",
            "args": ["server.js"],
            "env": {
                "DEBUG": "true"
            }
        }
    });
    fs::write(
        plugin_dir.join("mcp.json"),
        serde_json::to_string_pretty(&mcp_json).unwrap(),
    )
    .unwrap();

    let path = PluginPath::new(plugin_dir);
    let handle = PluginHandle::load(&path).unwrap();

    assert_eq!(handle.mcp_servers.len(), 1);
    assert_eq!(handle.mcp_servers[0].name, "example-server");
    assert_eq!(handle.mcp_servers[0].command, Some("node".to_string()));
    assert_eq!(handle.mcp_servers[0].args, vec!["server.js"]);
    assert_eq!(
        handle.mcp_servers[0].env.get("DEBUG"),
        Some(&"true".to_string())
    );
}

// =============================================================================
// Manifest Error Test
// =============================================================================

#[test]
fn test_load_manifest_not_found() {
    let temp = TempDir::new().unwrap();
    let empty_dir = temp.path().join("no-plugin");
    fs::create_dir_all(&empty_dir).unwrap();

    let path = PluginPath::new(empty_dir);
    let result = PluginHandle::load(&path);
    assert!(matches!(result, Err(PluginError::ManifestNotFound)));
}

// =============================================================================
// Accessor Tests
// =============================================================================

#[test]
fn test_get_plugin_by_name() {
    let temp = TempDir::new().unwrap();
    let plugin_path =
        create_test_plugin(temp.path(), "searchable-plugin", "1.0.0");

    let mut registry = PluginRegistry::new();
    registry.load_plugin(&plugin_path).unwrap();

    let found = registry.get_by_name("searchable-plugin");
    assert!(found.is_some());
    assert_eq!(found.unwrap().manifest.version, "1.0.0");

    let not_found = registry.get_by_name("nonexistent");
    assert!(not_found.is_none());
}
