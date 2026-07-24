//! Tests for summarizer.

use super::*;

#[test]
fn test_has_file_paths() {
    // Should detect Rust source files
    assert!(has_file_paths("src/main.rs"));
    assert!(has_file_paths("crates/synthia-agent/src/context/mod.rs"));
    assert!(has_file_paths("tests/integration_test.rs"));
    assert!(has_file_paths("examples/demo.rs"));
    assert!(has_file_paths("benches/benchmark.rs"));

    // Should detect config files
    assert!(has_file_paths("config.toml"));
    assert!(has_file_paths("package.json"));
    assert!(has_file_paths("settings.yaml"));
    assert!(has_file_paths("data.yml"));
    assert!(has_file_paths("README.md"));

    // Should detect home directory paths
    assert!(has_file_paths("/home/user/project/src/lib.rs"));
    assert!(has_file_paths("/usr/local/bin/executable"));

    // Should not match when only prefix or suffix exists
    assert!(!has_file_paths("src/"));
    assert!(!has_file_paths("just some text"));
    assert!(!has_file_paths("no file path here"));
}

#[test]
fn test_has_user_requests() {
    // Should detect user request keywords
    assert!(has_user_requests("user request"));
    assert!(has_user_requests("User Request"));
    assert!(has_user_requests("user asked for help"));
    assert!(has_user_requests("user wants to implement"));
    assert!(has_user_requests("user needs a new feature"));
    assert!(has_user_requests("requested by user"));
    assert!(has_user_requests("this is required"));
    assert!(has_user_requests("implement a new system"));
    assert!(has_user_requests("add a test"));
    assert!(has_user_requests("modify the config"));
    assert!(has_user_requests("update the documentation"));
    assert!(has_user_requests("delete the file"));
    assert!(has_user_requests("remove the old code"));
    assert!(has_user_requests("create a new module"));
    assert!(has_user_requests("fix the bug"));
    assert!(has_user_requests("change the behavior"));
    assert!(has_user_requests("please help"));
    assert!(has_user_requests("I want to start"));
    assert!(has_user_requests("I need to finish"));
    assert!(has_user_requests("I would like to continue"));

    // Should not match plain text without keywords
    assert!(!has_user_requests("The sky is blue."));
    assert!(!has_user_requests("This is a normal conversation."));
    assert!(!has_user_requests("The file contains data."));
}

#[test]
fn test_has_key_decisions() {
    assert!(has_key_decisions("we decided to go with A"));
    assert!(has_key_decisions("The decision was made"));
    assert!(has_key_decisions("in conclusion"));
    assert!(has_key_decisions("we agreed on the approach"));
    assert!(has_key_decisions("we chosen option B"));
    assert!(has_key_decisions("selected the fast path"));
    assert!(has_key_decisions("opted for simplicity"));
    assert!(has_key_decisions("determined to be correct"));
    assert!(has_key_decisions("resolved to proceed"));
    assert!(has_key_decisions("we will do it"));
    assert!(has_key_decisions("plan to implement"));
    assert!(has_key_decisions("going to refactor"));
    assert!(has_key_decisions("the approach is solid"));
    assert!(has_key_decisions("our strategy is clear"));
    assert!(has_key_decisions("the solution works well"));
    assert!(has_key_decisions("recommend using cache"));
    assert!(has_key_decisions("suggest trying again"));

    assert!(!has_key_decisions("The weather is nice."));
    assert!(!has_key_decisions("This file has content."));
    assert!(!has_key_decisions("Maybe we should consider."));
}

#[test]
fn test_check_summary_quality() {
    let good_summary = r#"
## Summary
This is a good summary with file path src/main.rs mentioned.

## User Intent
The user requested to implement a new feature.

## Current Work
We decided to use approach A for the solution.
"#;

    let quality = check_summary_quality(good_summary, &[]);
    assert!(quality.has_required_sections);
    assert!(quality.identifier_integrity);
    assert!(quality.user_request_reflected);
    assert!(quality.has_file_paths);
    assert!(quality.has_user_requests);
    assert!(quality.has_key_decisions);
    assert!(quality.overall_score >= 0.8);

    let bad_summary = "This is just a plain text without structure.";
    let quality = check_summary_quality(bad_summary, &[]);
    assert!(!quality.has_required_sections);
    assert!(quality.overall_score < 0.8);
}

#[test]
fn test_create_summary_message() {
    let summary = "Test summary content";
    let msg = create_summary_message(summary);

    let text = msg
        .content
        .extract_text()
        .expect("summary should have text");
    assert!(text.contains("## Summary of Previous Conversation"));
    assert!(text.contains(summary));
}