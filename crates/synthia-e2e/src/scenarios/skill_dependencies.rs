use std::{path::PathBuf, sync::Arc};

use synthia_core::Registry;
use synthia_skill::{
    registry::SkillRegistry,
    types::{
        Skill,
        SkillLevel,
        SkillLevels,
        SkillMetadata,
        SkillPaths,
        SkillSource,
        SkillState,
        SkillTokenCount,
    },
};

// ============================================================================
// Helper: Create a minimal skill for testing
// ============================================================================

fn make_skill(name: &str) -> Skill {
    Skill {
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

fn make_skill_with_deps(name: &str, depends_on: Vec<&str>) -> Skill {
    let mut skill = make_skill(name);
    skill.metadata.depends_on =
        depends_on.iter().map(|s| s.to_string()).collect();
    skill
}

fn make_skill_with_conflicts(name: &str, conflicts_with: Vec<&str>) -> Skill {
    let mut skill = make_skill(name);
    skill.metadata.conflicts_with =
        conflicts_with.iter().map(|s| s.to_string()).collect();
    skill
}

// ============================================================================
// E2E Test: Register skills with depends_on and activate with auto-dependency
// ============================================================================

#[tokio::test]
async fn test_skill_single_dependency_auto_activation() {
    let dir = tempfile::tempdir().unwrap();
    let registry = SkillRegistry::new(SkillPaths {
        user_dir: dir.path().to_path_buf(),
        project_dir: dir.path().to_path_buf(),
        builtin_dir: dir.path().to_path_buf(),
    });

    let base = make_skill("base-skill");
    let dependent = make_skill_with_deps("dependent-skill", vec!["base-skill"]);

    registry.register(base).await.unwrap();
    registry.register(dependent).await.unwrap();

    // Activate dependent - should auto-activate base
    registry.activate_skill("dependent-skill").unwrap();

    assert!(registry.is_active("base-skill"));
    assert!(registry.is_active("dependent-skill"));
}

// ============================================================================
// E2E Test: Transitive dependency chain (a <- b <- c)
// ============================================================================

#[tokio::test]
async fn test_skill_transitive_dependency() {
    let dir = tempfile::tempdir().unwrap();
    let registry = SkillRegistry::new(SkillPaths {
        user_dir: dir.path().to_path_buf(),
        project_dir: dir.path().to_path_buf(),
        builtin_dir: dir.path().to_path_buf(),
    });

    let a = make_skill("a");
    let b = make_skill_with_deps("b", vec!["a"]);
    let c = make_skill_with_deps("c", vec!["b"]);

    registry.register(a).await.unwrap();
    registry.register(b).await.unwrap();
    registry.register(c).await.unwrap();

    registry.activate_skill("c").unwrap();

    assert!(registry.is_active("a"));
    assert!(registry.is_active("b"));
    assert!(registry.is_active("c"));
}

// ============================================================================
// E2E Test: Circular dependency detection
// ============================================================================

#[tokio::test]
async fn test_skill_circular_dependency() {
    let dir = tempfile::tempdir().unwrap();
    let registry = SkillRegistry::new(SkillPaths {
        user_dir: dir.path().to_path_buf(),
        project_dir: dir.path().to_path_buf(),
        builtin_dir: dir.path().to_path_buf(),
    });

    let mut a = make_skill("circular-a");
    a.metadata.depends_on = vec!["circular-b".to_string()];
    let mut b = make_skill("circular-b");
    b.metadata.depends_on = vec!["circular-a".to_string()];

    registry.register(a).await.unwrap();
    registry.register(b).await.unwrap();

    let result = registry.activate_skill("circular-a");
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("circular"));
}

// ============================================================================
// E2E Test: Conflict detection
// ============================================================================

#[tokio::test]
async fn test_skill_conflict_with_active_skill() {
    let dir = tempfile::tempdir().unwrap();
    let registry = SkillRegistry::new(SkillPaths {
        user_dir: dir.path().to_path_buf(),
        project_dir: dir.path().to_path_buf(),
        builtin_dir: dir.path().to_path_buf(),
    });

    let a = make_skill("conflicting-a");
    let b = make_skill_with_conflicts("conflicting-b", vec!["conflicting-a"]);

    registry.register(a).await.unwrap();
    registry.register(b).await.unwrap();

    // Activate a first
    registry.activate_skill("conflicting-a").unwrap();

    // Activating b should fail because it conflicts with a
    let result = registry.activate_skill("conflicting-b");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("conflicts"));
}

// ============================================================================
// E2E Test: Conflicting skill can activate when conflict is inactive
// ============================================================================

#[tokio::test]
async fn test_skill_no_conflict_when_not_active() {
    let dir = tempfile::tempdir().unwrap();
    let registry = SkillRegistry::new(SkillPaths {
        user_dir: dir.path().to_path_buf(),
        project_dir: dir.path().to_path_buf(),
        builtin_dir: dir.path().to_path_buf(),
    });

    let a = make_skill("no-conflict-a");
    let b = make_skill_with_conflicts("no-conflict-b", vec!["no-conflict-a"]);

    registry.register(a).await.unwrap();
    registry.register(b).await.unwrap();

    // b should activate fine since a is not active
    registry.activate_skill("no-conflict-b").unwrap();
    assert!(registry.is_active("no-conflict-b"));
}

// ============================================================================
// E2E Test: Multiple dependencies activated in topological order
// ============================================================================

#[tokio::test]
async fn test_skill_multiple_dependencies() {
    let dir = tempfile::tempdir().unwrap();
    let registry = SkillRegistry::new(SkillPaths {
        user_dir: dir.path().to_path_buf(),
        project_dir: dir.path().to_path_buf(),
        builtin_dir: dir.path().to_path_buf(),
    });

    let api = make_skill("api-design");
    let git = make_skill("git-workflow");
    let web =
        make_skill_with_deps("web-dev", vec!["api-design", "git-workflow"]);

    registry.register(api).await.unwrap();
    registry.register(git).await.unwrap();
    registry.register(web).await.unwrap();

    registry.activate_skill("web-dev").unwrap();

    assert!(registry.is_active("api-design"));
    assert!(registry.is_active("git-workflow"));
    assert!(registry.is_active("web-dev"));
}

// ============================================================================
// E2E Test: Dependency not found returns error
// ============================================================================

#[tokio::test]
async fn test_skill_dependency_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let registry = SkillRegistry::new(SkillPaths {
        user_dir: dir.path().to_path_buf(),
        project_dir: dir.path().to_path_buf(),
        builtin_dir: dir.path().to_path_buf(),
    });

    let dependent =
        make_skill_with_deps("orphan-skill", vec!["nonexistent-base"]);

    registry.register(dependent).await.unwrap();

    let result = registry.activate_skill("orphan-skill");
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("nonexistent-base"));
}
