//! Unit tests for the skill registry.
//!
//! Kept in a separate `tests` submodule so the production code in
//! [`super::lifecycle`], [`super::query`], and [`super::registry_trait`]
//! stays uncluttered by test fixtures and `tempfile` dependencies.

use synthia_core::registry::{Registry, RegistryItem};

use super::types::SkillRegistry;
use crate::types::*;

fn make_skill(name: &str) -> crate::types::Skill {
    crate::types::Skill {
        metadata: SkillMetadata {
            name: name.to_string(),
            description: format!("{} description", name),
            triggers: vec![],
            priority: 0,
            license: None,
            compatibility: None,
            allowed_tools: vec![],
            exec: None,
            version: None,
            tags: vec![],
            metadata: Default::default(),
            levels: SkillLevels::default(),
            depends_on: vec![],
            conflicts_with: vec![],
        },
        body: String::new(),
        source: SkillSource::BuiltIn,
        level: SkillLevel::Level0,
        token_count: SkillTokenCount::default(),
        state: SkillState::Loaded,
    }
}

#[tokio::test]
async fn test_skill_registry_implements_registry_trait() {
    let dir = tempfile::tempdir().unwrap();
    let registry = SkillRegistry::new(SkillPaths {
        user_dir: dir.path().to_path_buf(),
        project_dir: dir.path().to_path_buf(),
        builtin_dir: dir.path().to_path_buf(),
    });

    let skill = make_skill("test-skill");
    let registered = registry.register(skill).await.unwrap();
    assert_eq!(registered.name(), "test-skill");
    assert_eq!(registered.description(), "test-skill description");

    let got = registry.get("test-skill").await.unwrap();
    assert!(got.is_some());
    assert_eq!(got.unwrap().name(), "test-skill");

    let missing = registry.get("nonexistent").await.unwrap();
    assert!(missing.is_none());

    let all = registry.list(None).await.unwrap();
    assert_eq!(all.len(), 1);

    let filtered = registry
        .list(Some(super::types::SkillFilter {
            source: Some(SkillSource::BuiltIn),
            tags: vec![],
            enabled_only: false,
        }))
        .await
        .unwrap();
    assert_eq!(filtered.len(), 1);

    let filtered_out = registry
        .list(Some(super::types::SkillFilter {
            source: Some(SkillSource::User),
            tags: vec![],
            enabled_only: false,
        }))
        .await
        .unwrap();
    assert!(filtered_out.is_empty());

    let duplicate = registry.register(make_skill("test-skill")).await;
    assert!(duplicate.is_err());

    Registry::<crate::types::Skill>::unregister(&registry, "test-skill")
        .await
        .unwrap();
    let after_unregister = registry.get("test-skill").await.unwrap();
    assert!(after_unregister.is_none());

    let not_found =
        Registry::<crate::types::Skill>::unregister(&registry, "nonexistent")
            .await;
    assert!(not_found.is_err());
}

#[tokio::test]
async fn test_skill_single_dependency() {
    let dir = tempfile::tempdir().unwrap();
    let registry = SkillRegistry::new(SkillPaths {
        user_dir: dir.path().to_path_buf(),
        project_dir: dir.path().to_path_buf(),
        builtin_dir: dir.path().to_path_buf(),
    });

    // Register base skill
    let base = make_skill("base-skill");
    registry.register(base).await.unwrap();

    // Register dependent skill with depends_on
    let mut dependent = make_skill("dependent-skill");
    dependent.metadata.depends_on = vec!["base-skill".to_string()];
    registry.register(dependent).await.unwrap();

    // Activate dependent - should auto-activate base
    registry.activate_skill("dependent-skill").unwrap();

    assert!(registry.is_active("base-skill"));
    assert!(registry.is_active("dependent-skill"));
}

#[tokio::test]
async fn test_skill_transitive_dependency() {
    let dir = tempfile::tempdir().unwrap();
    let registry = SkillRegistry::new(SkillPaths {
        user_dir: dir.path().to_path_buf(),
        project_dir: dir.path().to_path_buf(),
        builtin_dir: dir.path().to_path_buf(),
    });

    let a = make_skill("a");
    let mut b = make_skill("b");
    b.metadata.depends_on = vec!["a".to_string()];
    let mut c = make_skill("c");
    c.metadata.depends_on = vec!["b".to_string()];

    registry.register(a).await.unwrap();
    registry.register(b).await.unwrap();
    registry.register(c).await.unwrap();

    registry.activate_skill("c").unwrap();

    assert!(registry.is_active("a"));
    assert!(registry.is_active("b"));
    assert!(registry.is_active("c"));
}

#[tokio::test]
async fn test_skill_circular_dependency() {
    let dir = tempfile::tempdir().unwrap();
    let registry = SkillRegistry::new(SkillPaths {
        user_dir: dir.path().to_path_buf(),
        project_dir: dir.path().to_path_buf(),
        builtin_dir: dir.path().to_path_buf(),
    });

    let mut a = make_skill("a");
    a.metadata.depends_on = vec!["b".to_string()];
    let mut b = make_skill("b");
    b.metadata.depends_on = vec!["a".to_string()];

    registry.register(a).await.unwrap();
    registry.register(b).await.unwrap();

    let result = registry.activate_skill("a");
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("circular"));
}

#[tokio::test]
async fn test_skill_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let registry = SkillRegistry::new(SkillPaths {
        user_dir: dir.path().to_path_buf(),
        project_dir: dir.path().to_path_buf(),
        builtin_dir: dir.path().to_path_buf(),
    });

    let a = make_skill("conflicting-a");
    let mut b = make_skill("conflicting-b");
    b.metadata.conflicts_with = vec!["conflicting-a".to_string()];

    registry.register(a).await.unwrap();
    registry.register(b).await.unwrap();

    registry.activate_skill("conflicting-a").unwrap();
    let result = registry.activate_skill("conflicting-b");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("conflicts"));
}

#[tokio::test]
async fn test_skill_no_conflict_when_not_active() {
    let dir = tempfile::tempdir().unwrap();
    let registry = SkillRegistry::new(SkillPaths {
        user_dir: dir.path().to_path_buf(),
        project_dir: dir.path().to_path_buf(),
        builtin_dir: dir.path().to_path_buf(),
    });

    let a = make_skill("no-conflict-a");
    let mut b = make_skill("no-conflict-b");
    b.metadata.conflicts_with = vec!["no-conflict-a".to_string()];

    registry.register(a).await.unwrap();
    registry.register(b).await.unwrap();

    // b should activate fine since a is not active
    registry.activate_skill("no-conflict-b").unwrap();
    assert!(registry.is_active("no-conflict-b"));
}
