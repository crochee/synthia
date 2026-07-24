//! Unit tests for the `/skill` command.
//!
//! Covers every action verb in both registry-absent and
//! registry-present (integration) modes, including the
//! `--json` output shape for `list` / `info` / `stats` /
//! `report`. The integration tests use a `tempfile` tmpdir
//! with one fixture SKILL.md to exercise the real
//! `SkillRegistry::load_from_paths` path.

use std::sync::Arc;

use synthia_skill::{
    SkillRegistry,
    types::SkillPaths,
    usage::SkillUsageTracker,
};

use crate::{
    builtin::skill::SkillCommand,
    traits::CommandHandler,
    types::CommandContext,
};

fn create_test_skill_command() -> (SkillCommand, tempfile::TempDir) {
    let temp = tempfile::tempdir().unwrap();
    let skills_dir = temp.path().join("skills");
    let skill_dir = skills_dir.join("test-skill");
    std::fs::create_dir_all(&skill_dir).unwrap();

    let skill_content = r#"---
name: test-skill
description: A test skill for integration
triggers:
  - test
version: "1.0.0"
tags:
  - integration
allowed_tools:
  - write
  - read
---

This is the test skill body.
"#;
    std::fs::write(skill_dir.join("SKILL.md"), skill_content).unwrap();

    let paths = SkillPaths {
        user_dir: skills_dir.clone(),
        project_dir: temp.path().join("project"),
        builtin_dir: temp.path().join("builtin"),
    };
    let registry = Arc::new(SkillRegistry::new(paths));
    registry.load_from_paths(&[&skills_dir]).unwrap();

    let tracker = SkillUsageTracker::new();
    tracker.record_match("test-skill", 150);
    tracker.record_match("test-skill", 100);
    tracker.record_activation("test-skill", 500);

    let cmd = SkillCommand::with_registry(Arc::clone(&registry))
        .with_usage_tracker(tracker);
    (cmd, temp)
}

#[tokio::test]
async fn test_skill_no_args() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("", &ctx).await.unwrap();
    assert!(result.output.contains("Usage"));
    assert!(result.output.contains("list"));
    assert!(result.output.contains("info"));
    assert!(result.output.contains("validate"));
    assert!(result.output.contains("stats"));
    assert!(result.output.contains("report"));
}

#[tokio::test]
async fn test_skill_list_no_registry() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("list", &ctx).await.unwrap();
    assert!(result.output.contains("Available skills"));
}

#[tokio::test]
async fn test_skill_list_json_no_registry() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("list --json", &ctx).await.unwrap();
    assert_eq!(result.output, "[]");
}

#[tokio::test]
async fn test_skill_info_no_registry() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("info my-skill", &ctx).await.unwrap();
    assert!(result.output.contains("not found"));
}

#[tokio::test]
async fn test_skill_info_no_args() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("info", &ctx).await.unwrap();
    assert!(result.output.contains("Usage"));
}

#[tokio::test]
async fn test_skill_validate_nonexistent() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd
        .execute("validate /nonexistent/SKILL.md", &ctx)
        .await
        .unwrap();
    assert!(
        result.output.contains("not found")
            || result.output.contains("Valid: false")
    );
}

#[tokio::test]
async fn test_skill_validate_no_args() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("validate", &ctx).await.unwrap();
    assert!(result.output.contains("Usage"));
}

#[tokio::test]
async fn test_skill_stats_no_registry() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("stats", &ctx).await.unwrap();
    assert!(
        result.output.contains("Statistics")
            || result.output.contains("loaded_skills")
    );
}

#[tokio::test]
async fn test_skill_stats_json_no_registry() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("stats --json", &ctx).await.unwrap();
    assert!(result.output.contains("loaded_skills"));
    assert!(result.output.contains("total_token_usage"));
}

#[tokio::test]
async fn test_skill_report_no_tracker() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("report my-skill", &ctx).await.unwrap();
    assert!(
        result.output.contains("No usage tracker")
            || result.output.contains("No usage data")
    );
}

#[tokio::test]
async fn test_skill_report_no_args() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("report", &ctx).await.unwrap();
    assert!(result.output.contains("Usage"));
}

#[tokio::test]
async fn test_skill_enable_with_name() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("enable my-skill", &ctx).await.unwrap();
    assert!(result.output.contains("enabled"));
    assert!(result.output.contains("my-skill"));
}

#[tokio::test]
async fn test_skill_enable_without_name() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("enable", &ctx).await.unwrap();
    assert!(result.output.contains("Usage"));
}

#[tokio::test]
async fn test_skill_disable_with_name() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("disable my-skill", &ctx).await.unwrap();
    assert!(result.output.contains("disabled"));
    assert!(result.output.contains("my-skill"));
}

#[tokio::test]
async fn test_skill_disable_without_name() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("disable", &ctx).await.unwrap();
    assert!(result.output.contains("Usage"));
}

#[tokio::test]
async fn test_skill_install_no_installer() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("install /some/path.zip", &ctx).await.unwrap();
    assert!(result.output.contains("No installer configured"));
}

#[tokio::test]
async fn test_skill_install_no_args() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("install", &ctx).await.unwrap();
    assert!(result.output.contains("Usage"));
}

#[tokio::test]
async fn test_skill_uninstall_no_installer() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("uninstall my-skill", &ctx).await.unwrap();
    assert!(result.output.contains("No installer configured"));
}

#[tokio::test]
async fn test_skill_uninstall_no_args() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("uninstall", &ctx).await.unwrap();
    assert!(result.output.contains("Usage"));
}

#[tokio::test]
async fn test_skill_unknown_action() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("unknown", &ctx).await.unwrap();
    assert!(result.output.contains("Unknown skill action"));
}

#[tokio::test]
async fn test_integration_list_with_registry() {
    let (cmd, _temp) = create_test_skill_command();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("list", &ctx).await.unwrap();
    assert!(result.output.contains("test-skill"));
    assert!(result.output.contains("Available skills"));
}

#[tokio::test]
async fn test_integration_list_json_with_registry() {
    let (cmd, _temp) = create_test_skill_command();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("list --json", &ctx).await.unwrap();
    let skills: Vec<serde_json::Value> =
        serde_json::from_str(&result.output).unwrap();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0]["name"], "test-skill");
    assert!(skills[0]["source"].is_string());
    assert!(skills[0]["state"].is_string());
    assert!(skills[0]["token_count"].is_number());
}

#[tokio::test]
async fn test_integration_info_with_registry() {
    let (cmd, _temp) = create_test_skill_command();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("info test-skill", &ctx).await.unwrap();
    assert!(result.output.contains("test-skill"));
    assert!(result.output.contains("A test skill for integration"));
    assert!(result.output.contains("user"));
}

#[tokio::test]
async fn test_integration_info_json_with_registry() {
    let (cmd, _temp) = create_test_skill_command();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("info test-skill --json", &ctx).await.unwrap();
    let info: serde_json::Value = serde_json::from_str(&result.output).unwrap();
    assert_eq!(info["name"], "test-skill");
    assert_eq!(info["description"], "A test skill for integration");
    assert_eq!(info["version"], "1.0.0");
    assert_eq!(info["has_exec_scripts"], false);
    assert_eq!(info["triggers"].as_array().unwrap().len(), 1);
    assert_eq!(info["tags"].as_array().unwrap().len(), 1);
    assert!(info["token_count_level0"].as_u64().unwrap() > 0);
    assert!(info["token_count_level1"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn test_integration_info_not_found() {
    let (cmd, _temp) = create_test_skill_command();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("info nonexistent", &ctx).await.unwrap();
    assert!(result.output.contains("not found"));
}

#[tokio::test]
async fn test_integration_validate_valid_skill() {
    let (cmd, temp) = create_test_skill_command();
    let skill_path = temp.path().join("skills/test-skill/SKILL.md");
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd
        .execute(&format!("validate {}", skill_path.display()), &ctx)
        .await
        .unwrap();
    assert!(result.output.contains("Valid: true"));
}

#[tokio::test]
async fn test_integration_validate_invalid_file() {
    let cmd = SkillCommand::new();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd
        .execute("validate /no/such/file/SKILL.md", &ctx)
        .await
        .unwrap();
    assert!(result.output.contains("Valid: false"));
    assert!(result.output.contains("not found"));
}

#[tokio::test]
async fn test_integration_stats_with_registry() {
    let (cmd, _temp) = create_test_skill_command();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("stats", &ctx).await.unwrap();
    assert!(result.output.contains("Global Skill Statistics"));
    assert!(result.output.contains("Loaded skills: 1"));
}

#[tokio::test]
async fn test_integration_stats_json_with_registry() {
    let (cmd, _temp) = create_test_skill_command();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("stats --json", &ctx).await.unwrap();
    let stats: serde_json::Value =
        serde_json::from_str(&result.output).unwrap();
    assert_eq!(stats["loaded_skills"], 1);
    assert!(stats["total_token_usage"].is_number());
    assert!(stats["match_failure_rate"].is_number());
}

#[tokio::test]
async fn test_integration_report_with_tracker() {
    let (cmd, _temp) = create_test_skill_command();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("report test-skill", &ctx).await.unwrap();
    assert!(result.output.contains("Usage Report"));
    assert!(result.output.contains("Match count: 2"));
    assert!(result.output.contains("Activation count: 1"));
}

#[tokio::test]
async fn test_integration_report_json_with_tracker() {
    let (cmd, _temp) = create_test_skill_command();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("report test-skill --json", &ctx).await.unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&result.output).unwrap();
    assert_eq!(report["skill_name"], "test-skill");
    assert_eq!(report["match_count"], 2);
    assert_eq!(report["activation_count"], 1);
    assert_eq!(report["estimated_token_cost"], 750);
    assert!(report["last_matched"].is_string());
    assert!(report["last_activated"].is_string());
}

#[tokio::test]
async fn test_integration_report_unknown_skill() {
    let (cmd, _temp) = create_test_skill_command();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));
    let result = cmd.execute("report unknown-skill", &ctx).await.unwrap();
    assert!(result.output.contains("No usage data"));
}

#[tokio::test]
async fn test_integration_json_output_is_valid_json() {
    let (cmd, _temp) = create_test_skill_command();
    let ctx =
        CommandContext::new("s1".to_string(), std::path::PathBuf::from("/tmp"));

    let list_result = cmd.execute("list --json", &ctx).await.unwrap();
    serde_json::from_str::<Vec<serde_json::Value>>(&list_result.output)
        .expect("list --json should produce valid JSON");

    let info_result =
        cmd.execute("info test-skill --json", &ctx).await.unwrap();
    serde_json::from_str::<serde_json::Value>(&info_result.output)
        .expect("info --json should produce valid JSON");

    let stats_result = cmd.execute("stats --json", &ctx).await.unwrap();
    serde_json::from_str::<serde_json::Value>(&stats_result.output)
        .expect("stats --json should produce valid JSON");

    let report_result =
        cmd.execute("report test-skill --json", &ctx).await.unwrap();
    serde_json::from_str::<serde_json::Value>(&report_result.output)
        .expect("report --json should produce valid JSON");
}
