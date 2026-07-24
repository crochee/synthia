use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use parking_lot::RwLock;
use tempfile::TempDir;

use super::SkillWatcher;
use crate::{loader::SkillLoader, registry::SkillRegistry, types::SkillPaths};

fn create_test_skill_dir() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let skill_dir = dir.path().join("test-skill");
    fs::create_dir(&skill_dir).unwrap();

    let skill_content = r#"---
name: test-skill
description: A test skill for testing
priority: 0
triggers:
  - test
tags:
  - test
---

This is a test skill body.
"#;
    fs::write(skill_dir.join("SKILL.md"), skill_content).unwrap();
    dir
}

fn make_registry(skills_dir: &Path) -> Arc<RwLock<SkillRegistry>> {
    let paths = SkillPaths {
        builtin_dir: skills_dir.to_path_buf(),
        project_dir: PathBuf::new(),
        user_dir: PathBuf::new(),
    };
    let registry = SkillRegistry::new(paths);
    registry.load_from_paths(&[skills_dir]).unwrap();
    Arc::new(RwLock::new(registry))
}

#[tokio::test]
async fn test_watcher_detects_new_skill() {
    let temp_dir = create_test_skill_dir();
    let skills_dir = temp_dir.path().to_path_buf();

    let new_skill_dir = skills_dir.join("new-skill");
    fs::create_dir(&new_skill_dir).unwrap();

    let registry = make_registry(&skills_dir);
    let initial_count = registry.read().list_skills().len();

    let loader = Arc::new(SkillLoader);

    let mut watcher =
        SkillWatcher::new(skills_dir.clone(), Arc::clone(&registry), loader)
            .unwrap();
    watcher.start().unwrap();

    let new_skill_content = r#"---
name: new-skill
description: A new skill
priority: 0
triggers:
  - new
tags:
  - test
---

New skill body content.
"#;
    fs::write(new_skill_dir.join("SKILL.md"), new_skill_content).unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;

    let final_count = registry.read().list_skills().len();
    assert_eq!(
        final_count,
        initial_count + 1,
        "Expected one more skill after creation"
    );

    watcher.stop().unwrap();
}

#[tokio::test]
async fn test_watcher_detects_skill_modification() {
    let temp_dir = create_test_skill_dir();
    let skills_dir = temp_dir.path().to_path_buf();

    let registry = make_registry(&skills_dir);
    let loader = Arc::new(SkillLoader);

    let mut watcher =
        SkillWatcher::new(skills_dir.clone(), Arc::clone(&registry), loader)
            .unwrap();
    watcher.start().unwrap();

    let initial_skills = registry.read().list_skills();
    assert_eq!(initial_skills.len(), 1);
    assert_eq!(initial_skills[0].name, "test-skill");

    let modified_content = r#"---
name: test-skill
description: Modified description
priority: 1
triggers:
  - test
  - modified
tags:
  - test
---

Modified skill body.
"#;
    fs::write(skills_dir.join("test-skill/SKILL.md"), modified_content)
        .unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;

    let updated_skills = registry.read().list_skills();
    assert_eq!(updated_skills.len(), 1);
    assert_eq!(updated_skills[0].description, "Modified description");
    assert_eq!(updated_skills[0].priority, 1);

    watcher.stop().unwrap();
}

#[tokio::test]
async fn test_watcher_detects_skill_deletion() {
    let temp_dir = create_test_skill_dir();
    let skills_dir = temp_dir.path().to_path_buf();

    let registry = make_registry(&skills_dir);
    let loader = Arc::new(SkillLoader);

    let mut watcher =
        SkillWatcher::new(skills_dir.clone(), Arc::clone(&registry), loader)
            .unwrap();
    watcher.start().unwrap();

    assert_eq!(registry.read().list_skills().len(), 1);

    fs::remove_file(skills_dir.join("test-skill/SKILL.md")).unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;

    let final_count = registry.read().list_skills().len();
    assert_eq!(
        final_count, 0,
        "Expected skill to be removed after deletion"
    );

    watcher.stop().unwrap();
}

#[tokio::test]
async fn test_watcher_ignores_non_skill_md_files() {
    let temp_dir = create_test_skill_dir();
    let skills_dir = temp_dir.path().to_path_buf();

    let registry = make_registry(&skills_dir);
    let loader = Arc::new(SkillLoader);

    let mut watcher =
        SkillWatcher::new(skills_dir.clone(), Arc::clone(&registry), loader)
            .unwrap();
    watcher.start().unwrap();

    let other_file = skills_dir.join("test-skill/README.md");
    fs::write(&other_file, "Not a skill file").unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;

    let count = registry.read().list_skills().len();
    assert_eq!(
        count, 1,
        "Non-SKILL.md changes should not affect skill count"
    );

    watcher.stop().unwrap();
}
