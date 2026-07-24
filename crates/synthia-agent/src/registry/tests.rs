//! Unit tests for [`super::AgentRegistry`].
//!
//! Covers construction, the `Registry<AgentDefinition>` trait
//! (register / get / list with filter / unregister / already
//! exists), instance lifecycle (spawn / get / stop / stop_tree
//! / depth limit), `wrap_as_tool`, `load_from_path` from a
//! `TempDir` fixture, and the `RegistryItem` trait.

use std::path::{Path, PathBuf};

use chrono::Utc;
use synthia_core::{Error, registry::RegistryItem};
use synthia_tool::traits::Tool;
use tempfile::TempDir;

use crate::registry::{AgentRegistry, types::AgentFilter};

fn create_test_agent_dir(tmp_dir: &Path, name: &str) {
    let agent_dir = tmp_dir.join("agents").join(name);
    std::fs::create_dir_all(&agent_dir).unwrap();

    let metadata = format!(
        r#"name: {}
description: Test agent for unit tests
capabilities: ["test"]
when_to_use: ["testing"]
constraints: ["none"]
enabled: true
"#,
        name
    );
    std::fs::write(agent_dir.join("metadata.yaml"), metadata).unwrap();

    std::fs::write(
        agent_dir.join("SYSTEM.md"),
        "# System Prompt\nYou are a test agent.",
    )
    .unwrap();
}

fn make_def(
    id: &str,
    name: &str,
    enabled: bool,
    capabilities: Vec<&str>,
) -> crate::registry::types::AgentDefinition {
    crate::registry::types::AgentDefinition {
        id: id.to_string(),
        name: name.to_string(),
        description: format!("{} description", name),
        capabilities: capabilities.into_iter().map(String::from).collect(),
        when_to_use: vec![],
        constraints: vec![],
        system_prompt: "Test prompt".to_string(),
        source_path: PathBuf::from("/tmp"),
        file_hash: "abc123".to_string(),
        loaded_at: Utc::now(),
        enabled,
        permission_rules: vec![],
        permission_default: None,
        tools: None,
        denied_tools: None,
        extends: None,
        mode: None,
    }
}

#[test]
fn test_new_registry() {
    let registry = AgentRegistry::new();
    assert_eq!(registry.instance_count(), 0);
}

#[tokio::test]
async fn test_register_and_get() {
    use synthia_core::registry::Registry;
    let registry = AgentRegistry::new();
    let def = make_def("test-agent", "Test Agent", true, vec!["test"]);

    let result = registry.register(def.clone()).await;
    assert!(result.is_ok());

    let retrieved = registry.get("test-agent").await.unwrap();
    assert!(retrieved.is_some());
    assert!(retrieved.unwrap().id == "test-agent");
}

#[tokio::test]
async fn test_list_with_filter() {
    use synthia_core::registry::Registry;
    let registry = AgentRegistry::new();

    registry
        .register(make_def(
            "search-agent",
            "Search Agent",
            true,
            vec!["search"],
        ))
        .await
        .unwrap();
    registry
        .register(make_def("code-agent", "Code Agent", false, vec!["coding"]))
        .await
        .unwrap();

    let all = registry.list(None).await.unwrap();
    assert_eq!(all.len(), 2);

    let filter_enabled = AgentFilter {
        name: None,
        capability: None,
        enabled_only: true,
    };
    let enabled = registry.list(Some(filter_enabled)).await.unwrap();
    assert_eq!(enabled.len(), 1);

    let filter_cap = AgentFilter {
        name: None,
        capability: Some("search".to_string()),
        enabled_only: false,
    };
    let by_cap = registry.list(Some(filter_cap)).await.unwrap();
    assert_eq!(by_cap.len(), 1);
}

#[tokio::test]
async fn test_spawn_instance() {
    use synthia_core::registry::Registry;
    let registry = AgentRegistry::new();
    let def = make_def("test-agent", "Test Agent", true, vec!["test"]);
    registry.register(def).await.unwrap();

    let instance_id = registry.spawn("test-agent", None, None);
    assert!(instance_id.is_ok());

    let instance = registry.get_instance(&instance_id.unwrap());
    assert!(instance.is_some());
}

#[tokio::test]
async fn test_depth_limit() {
    use synthia_core::registry::Registry;
    let registry = AgentRegistry::with_max_depth(AgentRegistry::new(), 1);
    let def = make_def("test-agent", "Test Agent", true, vec!["test"]);
    registry.register(def).await.unwrap();

    let parent_id = registry.spawn("test-agent", None, None).unwrap();

    let child_result =
        registry.spawn("test-agent", Some(parent_id.clone()), None);
    assert!(child_result.is_ok());

    let grandchild_result =
        registry.spawn("test-agent", Some(child_result.unwrap()), None);
    assert!(grandchild_result.is_err());
    assert!(matches!(
        grandchild_result.unwrap_err(),
        Error::InvalidItem(_)
    ));
}

#[test]
fn test_load_from_path() {
    let tmp_dir = TempDir::new().unwrap();
    create_test_agent_dir(tmp_dir.path(), "test-agent");

    let registry = AgentRegistry::new();
    let count = registry.load_from_path(tmp_dir.path()).unwrap();

    assert_eq!(count, 1);

    let defs = registry.definitions.read();
    assert!(defs.contains_key("test-agent"));
}

#[tokio::test]
async fn test_stop_instance() {
    use synthia_core::registry::Registry;
    let registry = AgentRegistry::new();
    let def = make_def("test-agent", "Test Agent", true, vec!["test"]);
    registry.register(def).await.unwrap();

    let instance_id = registry.spawn("test-agent", None, None).unwrap();

    assert_eq!(registry.instance_count(), 1);

    registry.stop(&instance_id).await.unwrap();

    assert_eq!(registry.instance_count(), 0);
    assert!(registry.get_instance(&instance_id).is_none());
}

#[tokio::test]
async fn test_stop_tree() {
    use synthia_core::registry::Registry;
    let registry = AgentRegistry::new();
    let def = make_def("test-agent", "Test Agent", true, vec!["test"]);
    registry.register(def).await.unwrap();

    let parent_id = registry.spawn("test-agent", None, None).unwrap();
    let _child_id = registry
        .spawn("test-agent", Some(parent_id.clone()), None)
        .unwrap();

    assert_eq!(registry.instance_count(), 2);

    registry.stop_tree(&parent_id).await.unwrap();

    assert_eq!(registry.instance_count(), 0);
}

#[tokio::test]
async fn test_wrap_as_tool() {
    use synthia_core::registry::Registry;
    let registry = AgentRegistry::new();
    let def = make_def("test-agent", "Test Agent", true, vec!["test"]);
    registry.register(def).await.unwrap();

    let instance_id = registry.spawn("test-agent", None, None).unwrap();

    let wrapper = registry.wrap_as_tool(&instance_id);
    assert!(wrapper.is_ok());

    let wrapper = wrapper.unwrap();
    assert_eq!(wrapper.name(), "Test Agent");
    assert_eq!(wrapper.definition.id, "test-agent");
}

#[tokio::test]
async fn test_already_exists() {
    use synthia_core::registry::Registry;
    let registry = AgentRegistry::new();
    let def = make_def("test-agent", "Test Agent", true, vec!["test"]);

    registry.register(def.clone()).await.unwrap();
    let result = registry.register(def).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_contains_and_len() {
    use synthia_core::registry::Registry;
    let registry = AgentRegistry::new();

    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);

    let def = make_def("test-agent", "Test Agent", true, vec!["test"]);
    registry.register(def).await.unwrap();

    assert!(!registry.is_empty());
    assert_eq!(registry.len(), 1);
    assert!(registry.contains("test-agent"));
}

#[tokio::test]
async fn test_registry_item_trait() {
    let def = make_def("test-agent", "Test Agent", true, vec!["test"]);

    assert_eq!(def.name(), "Test Agent");
    assert_eq!(def.description(), "Test Agent description");
}
