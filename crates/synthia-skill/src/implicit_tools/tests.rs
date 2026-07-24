use std::sync::Arc;

use synthia_tool::{
    traits::Tool,
    types::{ToolExecutionContext, ToolInput},
};

use super::*;
use crate::{registry::SkillRegistry, types::SkillPaths};

fn make_registry() -> SkillRegistry {
    SkillRegistry::new(SkillPaths {
        user_dir: std::env::temp_dir(),
        project_dir: std::env::temp_dir(),
        builtin_dir: std::env::temp_dir(),
    })
}

#[test]
fn test_load_skill_tool_definition() {
    let def = load_skill_tool_definition();
    assert_eq!(def.name, "load_skill");
    assert!(def.description.contains("Load a skill"));
}

#[test]
fn test_unload_skill_tool_definition() {
    let def = unload_skill_tool_definition();
    assert_eq!(def.name, "unload_skill");
    assert!(def.description.contains("free up tokens"));
}

#[test]
fn test_inject_exec_scripts_empty() {
    let body = "Some body text";
    let result = inject_exec_scripts(body, &[]);
    assert_eq!(result, "Some body text");
}

#[test]
fn test_inject_exec_scripts_with_paths() {
    let body = "Some body text";
    let paths =
        vec!["scripts/setup.sh".to_string(), "scripts/run.sh".to_string()];
    let result = inject_exec_scripts(body, &paths);
    assert!(result.contains("### Available Scripts"));
    assert!(result.contains("`scripts/setup.sh`"));
    assert!(result.contains("`scripts/run.sh`"));
}

#[test]
fn test_create_implicit_tools() {
    let registry = Arc::new(make_registry());
    let (load_tool, unload_tool) = create_implicit_tools(Arc::clone(&registry));
    assert_eq!(load_tool.name(), "load_skill");
    assert_eq!(unload_tool.name(), "unload_skill");
}

#[tokio::test]
async fn test_load_skill_tool_missing_name_param() {
    let registry = Arc::new(make_registry());
    let tool = LoadSkillTool::new(registry);
    let input = ToolInput {
        name: "load_skill".to_string(),
        input: serde_json::json!({}),
        context: ToolExecutionContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        ),
    };
    let result = tool.call(input).await;
    assert!(result.is_error.unwrap_or(false));
    assert!(result.content[0].text().unwrap().contains("name"));
}

#[tokio::test]
async fn test_load_skill_tool_not_found() {
    let registry = Arc::new(make_registry());
    let tool = LoadSkillTool::new(registry);
    let input = ToolInput {
        name: "load_skill".to_string(),
        input: serde_json::json!({"name": "nonexistent_skill"}),
        context: ToolExecutionContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        ),
    };
    let result = tool.call(input).await;
    assert!(result.is_error.unwrap_or(false));
    assert!(result.content[0].text().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_unload_skill_tool_missing_name_param() {
    let registry = Arc::new(make_registry());
    let tool = UnloadSkillTool::new(registry);
    let input = ToolInput {
        name: "unload_skill".to_string(),
        input: serde_json::json!({}),
        context: ToolExecutionContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        ),
    };
    let result = tool.call(input).await;
    assert!(result.is_error.unwrap_or(false));
    assert!(result.content[0].text().unwrap().contains("name"));
}

#[tokio::test]
async fn test_unload_skill_tool_not_found() {
    let registry = Arc::new(make_registry());
    let tool = UnloadSkillTool::new(registry);
    let input = ToolInput {
        name: "unload_skill".to_string(),
        input: serde_json::json!({"name": "nonexistent_skill"}),
        context: ToolExecutionContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        ),
    };
    let result = tool.call(input).await;
    assert!(result.is_error.unwrap_or(false));
    assert!(result.content[0].text().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_unload_skill_noop_when_inactive() {
    let registry = Arc::new(make_registry());

    let tmp_dir = std::env::temp_dir()
        .join(format!("test_skill_noop_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let skill_md = tmp_dir.join("SKILL.md");
    let skill_name = tmp_dir.file_name().unwrap().to_str().unwrap();
    std::fs::write(
        &skill_md,
        format!(
            r#"---
name: {skill_name}
description: A test skill for no-op unloading
---

This is a test skill body with enough text to measure token count properly for testing purposes.
"#
        ),
    )
    .unwrap();

    registry.register_from_path(&tmp_dir).unwrap();

    let tool = UnloadSkillTool::new(registry);
    let input = ToolInput {
        name: "unload_skill".to_string(),
        input: serde_json::json!({"name": skill_name}),
        context: ToolExecutionContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        ),
    };
    let result = tool.call(input).await;
    assert!(!result.is_error.unwrap_or(false));
    assert!(
        result.content[0]
            .text()
            .unwrap()
            .contains("not currently loaded")
    );
    assert!(result.content[0].text().unwrap().contains("no-op"));

    let _ = std::fs::remove_dir_all(&tmp_dir);
}
