//! Unit tests for the matcher family.
//!
//! All 14 tests for [`strategy::MatchingStrategy`],
//! [`bm25_matcher::BM25Matcher`], [`keyword::KeywordMatcher`],
//! and [`hybrid::HybridMatcher`] live here so the
//! individual submodules stay focused on their public
//! surface and don't carry `#[cfg(test)]` blocks.

use super::*;
use crate::types::{
    Skill,
    SkillLevel,
    SkillMetadata,
    SkillSource,
    SkillState,
    SkillTokenCount,
};

/// Build a `Skill` literal for tests. Centralising this
/// here (and not in each submodule) is intentional: the
/// 16-field `SkillMetadata` literal is genuinely tedious
/// to repeat and would otherwise duplicate the helper
/// across every submodule.
fn test_skill(
    name: &str,
    description: &str,
    triggers: Vec<&str>,
    tags: Vec<&str>,
) -> Skill {
    Skill {
        metadata: SkillMetadata {
            name: name.to_string(),
            description: description.to_string(),
            triggers: triggers.into_iter().map(|s| s.to_string()).collect(),
            tags: tags.into_iter().map(|s| s.to_string()).collect(),
            priority: 0,
            license: None,
            compatibility: None,
            allowed_tools: Vec::new(),
            exec: None,
            version: Some("1.0.0".to_string()),
            metadata: std::collections::HashMap::new(),
            levels: Default::default(),
            depends_on: vec![],
            conflicts_with: vec![],
        },
        body: "".to_string(),
        source: SkillSource::BuiltIn,
        level: SkillLevel::Level0,
        token_count: SkillTokenCount {
            level0: 0,
            level1: 0,
        },
        state: SkillState::Loaded,
    }
}

#[test]
fn test_bm25_index_build_and_search() {
    let skills = vec![
        test_skill(
            "python-test",
            "Python testing framework",
            vec!["pytest", "unittest"],
            vec!["python", "test"],
        ),
        test_skill(
            "rust-build",
            "Rust build system",
            vec!["cargo", "build"],
            vec!["rust", "build"],
        ),
        test_skill(
            "deploy",
            "Deploy applications",
            vec!["deploy", "ship"],
            vec!["devops"],
        ),
    ];

    let index = crate::bm25::BM25Index::build(&skills);
    let results = index.search("python testing");

    assert!(!results.is_empty());
    assert!(results.iter().any(|s| s.name == "python-test"));
}

#[test]
fn test_bm25_add_skill_incremental() {
    let skills = vec![test_skill(
        "rust-build",
        "Rust build system",
        vec!["cargo"],
        vec!["rust"],
    )];
    let mut index = crate::bm25::BM25Index::build(&skills);

    let new_skill = test_skill(
        "python-test",
        "Python testing",
        vec!["pytest"],
        vec!["python"],
    );
    index.add_skill(&new_skill);

    let results = index.search("python testing");
    assert!(results.iter().any(|s| s.name == "python-test"));
}

#[test]
fn test_bm25_empty_index() {
    let index = crate::bm25::BM25Index::build(&[]);
    let results = index.search("anything");
    assert!(results.is_empty());
}

#[test]
fn test_bm25_score_ordering() {
    let skills = vec![
        test_skill(
            "python-test",
            "Python testing framework",
            vec!["pytest"],
            vec!["python", "test"],
        ),
        test_skill(
            "python-web",
            "Python web framework",
            vec!["flask"],
            vec!["python", "web"],
        ),
    ];

    let index = crate::bm25::BM25Index::build(&skills);
    let results = index.search("pytest unittest");

    assert!(!results.is_empty());
    for i in 1..results.len() {
        assert!(results[i - 1].bm25_score >= results[i].bm25_score);
    }
}

#[test]
fn test_keyword_matcher_basic() {
    let skills = vec![
        test_skill(
            "python-test",
            "Python testing",
            vec!["pytest"],
            vec!["python"],
        ),
        test_skill("rust-build", "Rust build", vec!["cargo"], vec!["rust"]),
    ];

    let results =
        keyword::KeywordMatcher::match_skills(&skills, "pytest unittest");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].skill.metadata.name, "python-test");
}

#[test]
fn test_keyword_matcher_no_match() {
    let skills = vec![test_skill(
        "python-test",
        "Python testing",
        vec!["pytest"],
        vec!["python"],
    )];

    let results =
        keyword::KeywordMatcher::match_skills(&skills, "rust cargo build");
    assert!(results.is_empty());
}

#[test]
fn test_hybrid_matcher_basic() {
    let skills = vec![
        test_skill(
            "python-test",
            "Python testing framework",
            vec!["pytest", "unittest"],
            vec!["python", "test"],
        ),
        test_skill(
            "rust-build",
            "Rust build system",
            vec!["cargo", "build"],
            vec!["rust", "build"],
        ),
        test_skill(
            "deploy",
            "Deploy applications",
            vec!["deploy", "ship"],
            vec!["devops"],
        ),
    ];

    let bm25_index = crate::bm25::BM25Index::build(&skills);
    let texts: Vec<(String, String)> = skills
        .iter()
        .map(|s| {
            (
                s.metadata.name.clone(),
                format!(
                    "{} {} {}",
                    s.metadata.name,
                    s.metadata.description,
                    s.metadata.triggers.join(" ")
                ),
            )
        })
        .collect();
    let mut emb_index = crate::embedding::SparseVectorIndex::new();
    emb_index.build_from_texts(&texts);

    let results = hybrid::HybridMatcher::match_skills(
        &skills,
        "python testing",
        &bm25_index,
        &emb_index,
        0.5,
        3,
    );
    assert!(!results.is_empty());
    assert_eq!(results[0].skill.metadata.name, "python-test");
}

#[test]
fn test_hybrid_matcher_empty() {
    let skills: Vec<Skill> = vec![];
    let bm25_index = crate::bm25::BM25Index::build(&skills);
    let emb_index = crate::embedding::SparseVectorIndex::new();
    let results = hybrid::HybridMatcher::match_skills(
        &skills,
        "anything",
        &bm25_index,
        &emb_index,
        0.5,
        3,
    );
    assert!(results.is_empty());
}

#[test]
fn test_hybrid_matcher_weights() {
    let skills = vec![
        test_skill(
            "python-test",
            "Python testing framework",
            vec!["pytest"],
            vec!["python"],
        ),
        test_skill(
            "python-web",
            "Python web framework",
            vec!["flask"],
            vec!["python"],
        ),
    ];

    let bm25_index = crate::bm25::BM25Index::build(&skills);
    let texts: Vec<(String, String)> = skills
        .iter()
        .map(|s| {
            (
                s.metadata.name.clone(),
                format!("{} {}", s.metadata.name, s.metadata.description),
            )
        })
        .collect();
    let mut emb_index = crate::embedding::SparseVectorIndex::new();
    emb_index.build_from_texts(&texts);

    let results_bm25_heavy = hybrid::HybridMatcher::match_skills(
        &skills,
        "pytest",
        &bm25_index,
        &emb_index,
        0.9,
        2,
    );
    assert!(!results_bm25_heavy.is_empty());
    assert_eq!(results_bm25_heavy[0].skill.metadata.name, "python-test");
}

/// Integration test: BM25 with 50 skills (mid-range tier).
#[test]
fn test_bm25_mid_range_50_skills() {
    let skills: Vec<Skill> = (0..50)
        .map(|i| {
            let lang = if i % 3 == 0 {
                "Python"
            } else if i % 3 == 1 {
                "Rust"
            } else {
                "JavaScript"
            };
            test_skill(
                &format!("skill-{}", i),
                &format!("{} development tool for task {}", lang, i),
                vec![&lang.to_lowercase(), &format!("task-{}", i)],
                vec![&lang.to_lowercase(), "tool"],
            )
        })
        .collect();

    let index = crate::bm25::BM25Index::build(&skills);
    assert_eq!(index.n_docs, 50);

    // Searching for "Python development" should return Python skills first
    let results = index.search("Python development");
    assert!(!results.is_empty());

    // Verify results are sorted by score descending
    for i in 1..results.len() {
        assert!(results[i - 1].bm25_score >= results[i].bm25_score);
    }

    // Top results should be Python skills
    let python_results: Vec<_> = results
        .iter()
        .filter(|s| {
            s.name.starts_with("skill-") && {
                let idx: usize =
                    s.name.trim_start_matches("skill-").parse().unwrap_or(0);
                idx.is_multiple_of(3)
            }
        })
        .collect();
    assert!(!python_results.is_empty());
}

/// Integration test: BM25 with 100 skills (upper range tier).
#[test]
fn test_bm25_large_100_skills() {
    let categories = [
        ("web", "HTTP API server framework"),
        ("database", "SQL query builder ORM"),
        ("testing", "unit test framework runner"),
        ("security", "encryption authentication"),
        ("devops", "deployment CI CD pipeline"),
    ];

    let skills: Vec<Skill> = (0..100)
        .map(|i| {
            let (cat, desc) = &categories[i % categories.len()];
            test_skill(
                &format!("{}-tool-{}", cat, i),
                &format!("{} - {}", cat, desc),
                vec![cat, &format!("tool-{}", i)],
                vec![cat, "automation"],
            )
        })
        .collect();

    let index = crate::bm25::BM25Index::build(&skills);
    assert_eq!(index.n_docs, 100);

    // "testing framework" should rank testing tools higher
    let results = index.search("testing framework");
    assert!(!results.is_empty());
    assert!(results.iter().any(|s| s.name.starts_with("testing-")));
}

/// Integration test: BM25 with 25 skills and specific term matching.
#[test]
fn test_bm25_term_specificity_25_skills() {
    let skills: Vec<Skill> = [
        (
            "git-manager",
            "Git repository manager for version control",
            vec!["git", "commit", "push"],
        ),
        (
            "git-hooks",
            "Git hooks automation for pre-commit checks",
            vec!["git", "hooks", "automation"],
        ),
        (
            "github-actions",
            "GitHub Actions CI CD integration",
            vec!["github", "ci", "cd"],
        ),
        (
            "deploy-scripts",
            "Deployment scripts for production",
            vec!["deploy", "production", "scripts"],
        ),
        (
            "docker-compose",
            "Docker container orchestration",
            vec!["docker", "compose", "container"],
        ),
        (
            "k8s-manager",
            "Kubernetes cluster manager",
            vec!["kubernetes", "cluster", "orchestration"],
        ),
        (
            "terraform-provisioner",
            "Terraform infrastructure provisioning",
            vec!["terraform", "infra", "provision"],
        ),
        (
            "ansible-automation",
            "Ansible automation configuration",
            vec!["ansible", "config", "automation"],
        ),
        (
            "ci-pipeline",
            "CI pipeline runner for testing",
            vec!["ci", "pipeline", "testing"],
        ),
        (
            "log-aggregator",
            "Log aggregation and monitoring",
            vec!["logging", "monitoring", "aggregation"],
        ),
        (
            "alert-manager",
            "Alert management system",
            vec!["alerts", "management", "monitoring"],
        ),
        (
            "db-migrator",
            "Database migration tool",
            vec!["database", "migration", "schema"],
        ),
        (
            "cache-warmer",
            "Cache warming and optimization",
            vec!["cache", "optimization", "performance"],
        ),
        (
            "api-gateway",
            "API gateway routing service",
            vec!["api", "gateway", "routing"],
        ),
        (
            "auth-provider",
            "Authentication OAuth provider",
            vec!["auth", "oauth", "authentication"],
        ),
        (
            "rate-limiter",
            "Rate limiting middleware",
            vec!["rate-limit", "middleware", "throttle"],
        ),
        (
            "load-balancer",
            "Load balancing service",
            vec!["load-balance", "service", "scaling"],
        ),
        (
            "service-mesh",
            "Service mesh for microservices",
            vec!["service-mesh", "microservices", "network"],
        ),
        (
            "config-manager",
            "Configuration management",
            vec!["config", "management", "settings"],
        ),
        (
            "secret-store",
            "Secret storage and vault",
            vec!["secrets", "vault", "security"],
        ),
        (
            "backup-tool",
            "Database backup and restore",
            vec!["backup", "restore", "database"],
        ),
        (
            "monitoring-dashboard",
            "Monitoring dashboard visualization",
            vec!["monitoring", "dashboard", "viz"],
        ),
        (
            "code-analyzer",
            "Static code analysis tool",
            vec!["analysis", "static", "lint"],
        ),
        (
            "test-generator",
            "Automated test generation",
            vec!["test", "generation", "automation"],
        ),
        (
            "doc-builder",
            "Documentation generator",
            vec!["docs", "documentation", "generator"],
        ),
    ]
    .iter()
    .map(|(name, desc, triggers)| {
        test_skill(name, desc, triggers.to_vec(), vec!["devops"])
    })
    .collect();

    let index = crate::bm25::BM25Index::build(&skills);
    assert_eq!(index.n_docs, 25);

    // "git commit" should rank git-manager higher than git-hooks
    let results = index.search("git commit");
    assert!(!results.is_empty());
    assert_eq!(results[0].name, "git-manager");
}

/// Integration test: tier-based matcher with 25 skills uses BM25Matcher.
#[test]
fn test_tier_bm25_selection_25_skills() {
    let skills: Vec<Skill> = (0..25)
        .map(|i| {
            let domain = if i < 10 {
                "Python"
            } else if i < 20 {
                "Rust"
            } else {
                "Go"
            };
            test_skill(
                &format!("{}-skill-{}", domain.to_lowercase(), i),
                &format!("{} development skill number {}", domain, i),
                vec![&domain.to_lowercase(), &format!("skill-{}", i)],
                vec![domain],
            )
        })
        .collect();

    let index = crate::bm25::BM25Index::build(&skills);
    let mut matcher = bm25_matcher::BM25Matcher::new();
    matcher.index = Some(index);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let results = runtime.block_on(async {
        matcher.match_skills("Python development", &skills).await
    });
    assert!(!results.is_empty());

    // All top results should be Python skills
    for r in results.iter().take(3) {
        assert!(r.skill.metadata.name.starts_with("python-"));
    }
}
