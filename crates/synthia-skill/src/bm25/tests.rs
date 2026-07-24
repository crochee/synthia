//! Unit tests for the BM25 skill-matching pipeline.
//!
//! Covers both [`super::index::BM25Index`] (build /
//! search / incremental update) and
//! [`super::matcher::BM25Matcher`] (priority-bonus
//! adjustment, threshold filtering).
//!
//! The original `bm25.rs` defined three near-duplicate
//! `test_skill*` helpers (`test_skill`,
//! `test_skill_with_priority`, `test_skill_with_source`).
//! They are collapsed into a single
//! [`make_skill`] + [`make_skill_with`] pair: the former
//! defaults to `priority=0, source=BuiltIn` (matching the
//! most common case), the latter accepts the full
//! override surface. The collapse removes ~90 lines of
//! copy-pasted struct literal.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{
        bm25::{BM25Index, BM25Matcher},
        types::{
            MatchStrategy,
            Skill,
            SkillLevel,
            SkillMetadata,
            SkillSource,
            SkillState,
            SkillTokenCount,
        },
    };

    /// Default-cased `make_skill`: `priority=0`,
    /// `source=BuiltIn`, empty `body` / `allowed_tools`
    /// / `depends_on` / `conflicts_with`.
    fn make_skill(
        name: &str,
        description: &str,
        triggers: Vec<&str>,
        tags: Vec<&str>,
    ) -> Skill {
        make_skill_with(
            name,
            description,
            triggers,
            tags,
            0,
            SkillSource::BuiltIn,
        )
    }

    /// Full-surface `make_skill` — used by the 2 tests
    /// that exercise `priority` or `source` variation.
    fn make_skill_with(
        name: &str,
        description: &str,
        triggers: Vec<&str>,
        tags: Vec<&str>,
        priority: i32,
        source: SkillSource,
    ) -> Skill {
        Skill {
            metadata: SkillMetadata {
                name: name.to_string(),
                description: description.to_string(),
                triggers: triggers.into_iter().map(String::from).collect(),
                tags: tags.into_iter().map(String::from).collect(),
                priority,
                license: None,
                compatibility: None,
                allowed_tools: Vec::new(),
                exec: None,
                version: Some("1.0.0".to_string()),
                metadata: HashMap::new(),
                levels: Default::default(),
                depends_on: vec![],
                conflicts_with: vec![],
            },
            body: String::new(),
            source,
            level: SkillLevel::Level0,
            token_count: SkillTokenCount {
                level0: 0,
                level1: 0,
            },
            state: SkillState::Loaded,
        }
    }

    // --- BM25Index build tests ---

    #[test]
    fn test_bm25_index_build_and_search() {
        let skills = vec![
            make_skill(
                "python-test",
                "Python testing framework",
                vec!["pytest", "unittest"],
                vec!["python", "test"],
            ),
            make_skill(
                "rust-build",
                "Rust build system",
                vec!["cargo", "build"],
                vec!["rust", "build"],
            ),
            make_skill(
                "deploy",
                "Deploy applications",
                vec!["deploy", "ship"],
                vec!["devops"],
            ),
        ];

        let index = BM25Index::build(&skills);
        assert_eq!(index.n_docs, 3);

        let results = index.search("python testing");
        assert!(!results.is_empty());
        assert!(results.iter().any(|s| s.name == "python-test"));
    }

    #[test]
    fn test_bm25_empty_index() {
        let index = BM25Index::build(&[]);
        assert_eq!(index.n_docs, 0);
        assert_eq!(index.avg_dl, 0.0);
        let results = index.search("anything");
        assert!(results.is_empty());
    }

    #[test]
    fn test_bm25_empty_query() {
        let skills = vec![make_skill("test", "test", vec!["test"], vec![])];
        let index = BM25Index::build(&skills);
        let results = index.search("");
        assert!(results.is_empty());
    }

    // --- BM25Index search tests ---

    #[test]
    fn test_bm25_score_ordering() {
        let skills = vec![
            make_skill(
                "python-test",
                "Python testing framework",
                vec!["pytest"],
                vec!["python", "test"],
            ),
            make_skill(
                "python-web",
                "Python web framework",
                vec!["flask"],
                vec!["python", "web"],
            ),
        ];

        let index = BM25Index::build(&skills);
        let results = index.search("pytest unittest");

        assert!(!results.is_empty());
        for i in 1..results.len() {
            assert!(results[i - 1].bm25_score >= results[i].bm25_score);
        }
    }

    #[test]
    fn test_bm25_no_match_returns_empty() {
        let skills = vec![make_skill(
            "python-test",
            "Python testing framework",
            vec!["pytest"],
            vec!["python"],
        )];

        let index = BM25Index::build(&skills);
        let results = index.search("kubernetes deployment");
        assert!(results.is_empty());
    }

    #[test]
    fn test_bm25_case_insensitive() {
        let skills = vec![make_skill(
            "python-test",
            "Python testing framework",
            vec!["pytest"],
            vec!["python"],
        )];

        let index = BM25Index::build(&skills);
        let results_lower = index.search("python testing");
        let results_upper = index.search("PYTHON TESTING");

        assert_eq!(results_lower.len(), results_upper.len());
        if !results_lower.is_empty() {
            assert_eq!(results_lower[0].name, results_upper[0].name);
            assert!(
                (results_lower[0].bm25_score - results_upper[0].bm25_score)
                    .abs()
                    < 1e-10
            );
        }
    }

    #[test]
    fn test_bm25_multiple_terms_score_higher() {
        let skills = vec![
            make_skill(
                "git-manager",
                "Git repository manager for version control",
                vec!["git", "commit", "push"],
                vec!["devops"],
            ),
            make_skill(
                "git-hooks",
                "Git hooks automation for pre-commit checks",
                vec!["git", "hooks", "automation"],
                vec!["devops"],
            ),
            make_skill(
                "deploy",
                "Deployment scripts for production",
                vec!["deploy"],
                vec!["devops"],
            ),
        ];

        let index = BM25Index::build(&skills);
        let results = index.search("git commit");

        assert!(!results.is_empty());
        assert_eq!(results[0].name, "git-manager");
    }

    // --- BM25Index incremental tests ---

    #[test]
    fn test_bm25_add_skill_incremental() {
        let skills = vec![make_skill(
            "rust-build",
            "Rust build system",
            vec!["cargo"],
            vec!["rust"],
        )];
        let mut index = BM25Index::build(&skills);
        assert_eq!(index.n_docs, 1);

        let new_skill = make_skill(
            "python-test",
            "Python testing",
            vec!["pytest"],
            vec!["python"],
        );
        index.add_skill(&new_skill);

        assert_eq!(index.n_docs, 2);
        let results = index.search("python testing");
        assert!(results.iter().any(|s| s.name == "python-test"));
    }

    #[test]
    fn test_bm25_rebuild() {
        let skills = vec![make_skill(
            "rust-build",
            "Rust build system",
            vec!["cargo"],
            vec!["rust"],
        )];
        let mut index = BM25Index::build(&skills);

        let new_skills = vec![make_skill(
            "python-test",
            "Python testing",
            vec!["pytest"],
            vec!["python"],
        )];
        index.rebuild(&new_skills);

        assert_eq!(index.n_docs, 1);
        let results = index.search("python testing");
        assert!(results.iter().any(|s| s.name == "python-test"));
        assert!(!results.iter().any(|s| s.name == "rust-build"));
    }

    #[test]
    fn test_bm25_new_default() {
        let index = BM25Index::new();
        assert_eq!(index.n_docs, 0);
        assert_eq!(index.avg_dl, 0.0);
        assert!((index.k1 - 1.2).abs() < 1e-10);
        assert!((index.b - 0.75).abs() < 1e-10);
    }

    #[test]
    fn test_bm25_default_trait() {
        let index = BM25Index::default();
        assert_eq!(index.n_docs, 0);
    }

    // --- BM25Matcher tests ---

    #[test]
    fn test_bm25_matcher_basic() {
        let skills = vec![
            make_skill(
                "python-test",
                "Python testing framework",
                vec!["pytest"],
                vec!["python"],
            ),
            make_skill(
                "rust-build",
                "Rust build system",
                vec!["cargo"],
                vec!["rust"],
            ),
        ];

        let index = BM25Index::build(&skills);
        let results =
            BM25Matcher::match_skills(&skills, "python testing", &index);

        assert!(!results.is_empty());
        assert_eq!(results[0].skill.metadata.name, "python-test");
        assert!(results[0].final_score > 0.0);
    }

    #[test]
    fn test_bm25_matcher_no_results() {
        let skills = vec![make_skill(
            "rust-build",
            "Rust build system",
            vec!["cargo"],
            vec!["rust"],
        )];

        let index = BM25Index::build(&skills);
        let results =
            BM25Matcher::match_skills(&skills, "python testing", &index);
        assert!(results.is_empty());
    }

    #[test]
    fn test_bm25_matcher_empty_skills() {
        let skills: Vec<Skill> = vec![];
        let index = BM25Index::build(&skills);
        let results = BM25Matcher::match_skills(&skills, "anything", &index);
        assert!(results.is_empty());
    }

    #[test]
    fn test_bm25_matcher_priority_bonus() {
        let skill_low = make_skill_with(
            "low-pri",
            "Python testing framework",
            vec!["pytest"],
            vec!["python"],
            0,
            SkillSource::BuiltIn,
        );
        let skill_high = make_skill_with(
            "high-pri",
            "Python testing framework",
            vec!["pytest"],
            vec!["python"],
            5,
            SkillSource::BuiltIn,
        );
        let skills = vec![skill_low, skill_high];

        let index = BM25Index::build(&skills);
        let results =
            BM25Matcher::match_skills(&skills, "python testing", &index);

        assert_eq!(results.len(), 2);
        // Both match the same BM25 terms, so raw scores should be equal
        let low = results
            .iter()
            .find(|r| r.skill.metadata.name == "low-pri")
            .unwrap();
        let high = results
            .iter()
            .find(|r| r.skill.metadata.name == "high-pri")
            .unwrap();
        assert!((low.bm25_score - high.bm25_score).abs() < 1e-10);
        // high-pri should have higher final_score due to priority bonus
        assert!(high.final_score > low.final_score);
        // Verify the bonus formula: final_score = bm25_score * (1 + priority * 0.1)
        let expected_high = high.bm25_score * (1.0 + 5.0 * 0.1);
        assert!((high.final_score - expected_high).abs() < 1e-10);
    }

    #[test]
    fn test_bm25_matcher_source_tiebreaker() {
        // Same description, different sources - all priorities 0
        let skills = vec![
            make_skill_with(
                "builtin-tool",
                "A tool description same text",
                vec!["tool"],
                vec!["test"],
                0,
                SkillSource::BuiltIn,
            ),
            make_skill_with(
                "project-tool",
                "A tool description same text",
                vec!["tool"],
                vec!["test"],
                0,
                SkillSource::Project,
            ),
            make_skill_with(
                "user-tool",
                "A tool description same text",
                vec!["tool"],
                vec!["test"],
                0,
                SkillSource::User,
            ),
        ];

        let index = BM25Index::build(&skills);
        let results = BM25Matcher::match_skills(&skills, "tool", &index);
        assert_eq!(results.len(), 3);

        // All have same bm25_score and priority 0, so final_score is equal.
        // BM25Matcher itself does NOT apply source tiebreaker - the registry does.
        // Verify all bm25_scores are equal
        for i in 1..results.len() {
            assert!(
                (results[i - 1].bm25_score - results[i].bm25_score).abs()
                    < 1e-10
            );
        }
    }

    #[test]
    fn test_bm25_matcher_matched_by_strategy() {
        let skills = vec![make_skill(
            "python-test",
            "Python testing framework",
            vec!["pytest"],
            vec!["python"],
        )];

        let index = BM25Index::build(&skills);
        let results =
            BM25Matcher::match_skills(&skills, "python testing", &index);

        assert!(!results.is_empty());
        assert!(matches!(results[0].matched_by, MatchStrategy::BM25));
    }

    #[test]
    fn test_bm25_matcher_threshold_filtering() {
        let skills = vec![
            make_skill(
                "python-test",
                "Python testing framework",
                vec!["pytest", "unittest"],
                vec!["python", "test"],
            ),
            make_skill(
                "rust-build",
                "Rust build system",
                vec!["cargo", "build"],
                vec!["rust", "build"],
            ),
            make_skill(
                "deploy",
                "Deploy applications",
                vec!["deploy", "ship"],
                vec!["devops"],
            ),
        ];

        let index = BM25Index::build(&skills);

        let all_results =
            BM25Matcher::match_skills(&skills, "python testing", &index);
        assert!(!all_results.is_empty());

        let thresholded_results = BM25Matcher::match_skills_with_threshold(
            &skills,
            "python testing",
            &index,
            5.0,
        );
        for r in &thresholded_results {
            assert!(r.bm25_score >= 5.0);
        }
    }

    #[test]
    fn test_bm25_matcher_high_threshold_returns_none() {
        let skills = vec![make_skill(
            "python-test",
            "Python testing framework",
            vec!["pytest"],
            vec!["python"],
        )];

        let index = BM25Index::build(&skills);
        let results = BM25Matcher::match_skills_with_threshold(
            &skills,
            "python testing",
            &index,
            1000.0,
        );
        assert!(results.is_empty());
    }

    #[test]
    fn test_bm25_matcher_zero_threshold_returns_all() {
        let skills = vec![
            make_skill(
                "python-test",
                "Python testing framework",
                vec!["pytest"],
                vec!["python"],
            ),
            make_skill(
                "rust-build",
                "Rust build system",
                vec!["cargo"],
                vec!["rust"],
            ),
        ];

        let index = BM25Index::build(&skills);

        let all_results =
            BM25Matcher::match_skills(&skills, "python testing", &index);
        let zero_threshold_results = BM25Matcher::match_skills_with_threshold(
            &skills,
            "python testing",
            &index,
            0.0,
        );

        assert_eq!(all_results.len(), zero_threshold_results.len());
    }
}
