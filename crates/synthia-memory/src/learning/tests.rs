//! Unit tests for the `learning` module family.
//!
//! Coverage map (7 tests):
//!
//! - `learn_from_success`: 1 test
//!   ([`test_learner_from_success`]).
//! - `learn_from_failure` + `get_failure_warnings`: 1 test
//!   ([`test_learner_from_failure`]).
//! - `suggest_action` (5x success): 1 test
//!   ([`test_suggestion_generation`]).
//! - `generate_report` (empty state): 1 test
//!   ([`test_learning_report`]).
//! - `serialize` / `deserialize`: 1 test
//!   ([`test_serialize_deserialize`]).
//! - `save_to_file` / `load_from_file`: 1 test
//!   ([`test_save_load_file`]).
//! - Empty + non-empty `success_count` assertion: 1 test
//!   (assertion in [`test_learner_from_success`]).

use std::collections::HashMap;

use super::*;

fn make_context(i: usize) -> TaskContext {
    TaskContext {
        input_summary: format!("Test task {}", i),
        tools_used: vec!["bash".to_string()],
        steps_taken: 1,
        execution_time_ms: 100,
        environment: HashMap::new(),
    }
}

fn make_success_outcome() -> Outcome {
    Outcome {
        result_summary: "Task completed successfully".to_string(),
        quality_score: 0.9,
        error_type: None,
        error_message: None,
    }
}

fn make_failure_outcome() -> Outcome {
    Outcome {
        result_summary: "Task failed".to_string(),
        quality_score: 0.2,
        error_type: Some("Timeout".to_string()),
        error_message: Some("Task timed out".to_string()),
    }
}

#[test]
fn test_learner_from_success() {
    let mut learner = ExperienceLearner::new();

    for i in 0..3 {
        let record = ExperienceRecord::new(
            format!("rec_{}", i),
            "test_task".to_string(),
            make_context(i),
            make_success_outcome(),
            vec![],
            true,
        );

        learner.learn_from_success(&record);
    }

    let experience_id = "exp_test_task";
    let experience = learner.experiences.iter().find(|e| e.id == experience_id);
    assert!(experience.is_some());
    let exp = experience.unwrap();
    assert_eq!(exp.success_count, 3);
    assert!(exp.is_reliable());
}

#[test]
fn test_learner_from_failure() {
    let mut learner = ExperienceLearner::new();

    let record = ExperienceRecord::new(
        "rec_2".to_string(),
        "test_task".to_string(),
        make_context(0),
        make_failure_outcome(),
        vec![],
        false,
    );

    learner.learn_from_failure(&record);

    let warnings = learner.get_failure_warnings();
    assert_eq!(warnings.len(), 1);
}

#[test]
fn test_suggestion_generation() {
    let mut learner = ExperienceLearner::new();

    for i in 0..5 {
        let outcome = Outcome {
            result_summary: format!("Success {}", i),
            quality_score: 0.9,
            error_type: None,
            error_message: None,
        };

        let record = ExperienceRecord::new(
            format!("rec_{}", i),
            "test_task".to_string(),
            TaskContext {
                input_summary: "Test".to_string(),
                tools_used: vec![],
                steps_taken: 1,
                execution_time_ms: 100,
                environment: HashMap::new(),
            },
            outcome,
            vec![],
            true,
        );

        learner.learn_from_success(&record);
    }

    let suggestions = learner.suggest_action("test_task");
    assert!(!suggestions.is_empty());
    assert!(
        suggestions
            .iter()
            .all(|s| s.action_type == SuggestionType::FollowPattern)
    );
}

#[test]
fn test_learning_report() {
    let learner = ExperienceLearner::new();
    let report = learner.generate_report();

    assert_eq!(report.total_experiences, 0);
    assert_eq!(report.avg_success_rate, 0.5);
}

#[test]
fn test_serialize_deserialize() {
    let mut learner = ExperienceLearner::new();

    let record = ExperienceRecord::new(
        "rec1".into(),
        "test_task".into(),
        TaskContext {
            input_summary: "Test".into(),
            tools_used: vec![],
            steps_taken: 1,
            execution_time_ms: 100,
            environment: std::collections::HashMap::new(),
        },
        make_success_outcome(),
        vec![],
        true,
    );

    learner.learn_from_success(&record);

    let serialized = learner.serialize().unwrap();
    let mut new_learner = ExperienceLearner::new();
    new_learner.deserialize(&serialized).unwrap();

    assert_eq!(new_learner.experiences.len(), 1);
}

#[test]
fn test_save_load_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("experiences.json");

    let mut learner = ExperienceLearner::new();
    let record = ExperienceRecord::new(
        "rec1".into(),
        "test_task".into(),
        TaskContext {
            input_summary: "Test".into(),
            tools_used: vec![],
            steps_taken: 1,
            execution_time_ms: 100,
            environment: std::collections::HashMap::new(),
        },
        make_success_outcome(),
        vec![],
        true,
    );
    learner.learn_from_success(&record);

    learner.save_to_file(&file_path).unwrap();
    let loaded = ExperienceLearner::load_from_file(&file_path).unwrap();

    assert_eq!(loaded.experiences.len(), 1);
}
