//! Unit tests for the `skill_cmd` family.
//!
//! All 19 tests for the discover / view / validate /
//! lifecycle / report entry points live here so each
//! submodule stays focused on its public surface and
//! doesn't carry a `#[cfg(test)]` block.

use std::{fs, path::Path};

use synthia_skill::types::SkillSource;
use tempfile::TempDir;

use super::{
    discover::source_to_string,
    lifecycle::{install_skill, uninstall_skill},
    report::{show_skill_report, show_skill_stats},
    validate::validate_skill,
    view::{list_installed_skills, list_skills, show_skill_info},
};

/// Write a `SKILL.md` with valid frontmatter into
/// `<dir>/<name>/SKILL.md`. Centralised because 9 of
/// the 19 tests need this fixture.
fn create_test_skill(dir: &Path, name: &str, body: &str) {
    let skill_dir = dir.join(name);
    fs::create_dir_all(&skill_dir).unwrap();
    let content = format!(
        "---\nname: {}\ndescription: Test skill {}\ntriggers:\n  - test\nversion: \"1.0.0\"\ntags:\n  - test\n---\n{}",
        name, name, body
    );
    fs::write(skill_dir.join("SKILL.md"), content).unwrap();
}

#[test]
fn test_validate_valid_skill() {
    let dir = TempDir::new().unwrap();
    let skill_path = dir.path().join("SKILL.md");
    let content = "---\nname: test-skill\ndescription: A test skill\ntriggers:\n  - test\ntags:\n  - test\n---\nSome body content.";
    fs::write(&skill_path, content).unwrap();

    validate_skill(&skill_path, false).unwrap();
}

#[test]
fn test_validate_missing_frontmatter() {
    let dir = TempDir::new().unwrap();
    let skill_path = dir.path().join("SKILL.md");
    fs::write(&skill_path, "No frontmatter here").unwrap();

    validate_skill(&skill_path, false).unwrap();
}

#[test]
fn test_validate_json_output() {
    let dir = TempDir::new().unwrap();
    let skill_path = dir.path().join("SKILL.md");
    let content = "---\nname: test-skill\ndescription: A test skill\ntriggers:\n  - test\n---\nBody content.";
    fs::write(&skill_path, content).unwrap();

    validate_skill(&skill_path, true).unwrap();
}

#[test]
fn test_validate_missing_name_field() {
    let dir = TempDir::new().unwrap();
    let skill_path = dir.path().join("SKILL.md");
    let content = "---\ndescription: No name\n---\nBody";
    fs::write(&skill_path, content).unwrap();

    validate_skill(&skill_path, false).unwrap();
}

#[test]
fn test_validate_file_not_found_json() {
    let dir = TempDir::new().unwrap();
    let skill_path = dir.path().join("nonexistent.md");

    validate_skill(&skill_path, true).unwrap();
}

#[test]
fn test_validate_warnings_for_missing_optional_fields() {
    let dir = TempDir::new().unwrap();
    let skill_path = dir.path().join("SKILL.md");
    let content = "---\nname: minimal\ndescription: Minimal skill\n---\nBody.";
    fs::write(&skill_path, content).unwrap();

    validate_skill(&skill_path, false).unwrap();
}

#[test]
fn test_list_skills_json_format() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();

    let user_skills = workspace.join(".agents/skills");
    create_test_skill(&user_skills, "skill-a", "Body A");
    create_test_skill(&user_skills, "skill-b", "Body B");

    list_skills(workspace, true).unwrap();
}

#[test]
fn test_show_skill_info_not_found() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();

    let result = show_skill_info(workspace, "nonexistent", false);
    assert!(result.is_err());
}

#[test]
fn test_source_to_string() {
    assert_eq!(source_to_string(&SkillSource::BuiltIn), "builtin");
    assert_eq!(source_to_string(&SkillSource::Project), "project");
    assert_eq!(source_to_string(&SkillSource::User), "user");
}

#[test]
fn test_show_skill_info_json_format() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();

    let user_skills = workspace.join(".agents/skills");
    create_test_skill(&user_skills, "json-test", "Body content");

    show_skill_info(workspace, "json-test", true).unwrap();
}

#[test]
fn test_validate_empty_body_warning() {
    let dir = TempDir::new().unwrap();
    let skill_path = dir.path().join("SKILL.md");
    let content = "---\nname: empty-body\ndescription: Has empty body\ntriggers: [test]\ntags: [test]\n---\n";
    fs::write(&skill_path, content).unwrap();

    validate_skill(&skill_path, true).unwrap();
}

#[test]
fn test_list_skills_empty_workspace() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();

    list_skills(workspace, false).unwrap();
}

#[test]
fn test_show_skill_info_human_readable() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();

    let user_skills = workspace.join(".agents/skills");
    create_test_skill(
        &user_skills,
        "human-info",
        "This is the skill body content for testing purposes.",
    );

    show_skill_info(workspace, "human-info", false).unwrap();
}

#[test]
fn test_skill_stats_empty_workspace() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();

    show_skill_stats(workspace, false).unwrap();
}

#[test]
fn test_skill_stats_json_format() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();

    let user_skills = workspace.join(".agents/skills");
    create_test_skill(&user_skills, "stat-test", "Body content");

    show_skill_stats(workspace, true).unwrap();
}

#[test]
fn test_skill_report_not_found() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();

    let result = show_skill_report(workspace, "nonexistent-skill", false);
    assert!(result.is_err());
}

#[test]
fn test_skill_report_json_format() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();

    let user_skills = workspace.join(".agents/skills");
    create_test_skill(&user_skills, "report-test", "Body content");

    show_skill_report(workspace, "report-test", true).unwrap();
}

#[test]
fn test_skill_report_human_readable() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();

    let user_skills = workspace.join(".agents/skills");
    create_test_skill(
        &user_skills,
        "report-human",
        "Body content for testing report output.",
    );

    show_skill_report(workspace, "report-human", false).unwrap();
}

// Smoke tests for install / uninstall to make sure
// the dispatch through `lifecycle.rs` doesn't panic on
// the "archive not found" / "skill not installed" paths
// we test elsewhere.
#[test]
fn test_install_skill_archive_not_found() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();
    let missing = dir.path().join("does-not-exist.skill");

    let result = install_skill(workspace, &missing, None);
    assert!(result.is_err());
}

#[test]
fn test_uninstall_skill_not_found() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();

    // Uninstall from a workspace with no skills — the
    // call will build a registry/installer that has
    // zero install records; we just need it to NOT
    // panic, and to bubble up the "no such skill"
    // error from the installer.
    let result = uninstall_skill(workspace, "does-not-exist");
    assert!(result.is_err());
}

#[test]
fn test_list_installed_skills_empty() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();

    list_installed_skills(workspace, false).unwrap();
}
