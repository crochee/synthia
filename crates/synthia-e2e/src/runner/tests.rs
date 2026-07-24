//! Unit tests for the runner family.
//!
//! All 8 tests for [`super::types::TestResult`],
//! [`super::types::TestStatus`], [`super::scenarios`]
//! (one per public entry point), and
//! [`super::junit::write_junit_xml`] /
//! [`super::junit::escape_xml`] live here. The
//! [`super::run`] functions aren't tested here —
//! they're thin orchestrators over the others and
//! the underlying scenarios / `write_junit_xml` are
//! already exercised.

use super::{
    junit::{escape_xml, write_junit_xml},
    scenarios::{
        benchmark_performance,
        test_basic_qa,
        test_error_recovery,
        test_guardian_enforcement,
        test_multi_turn,
        test_rate_limit_simulation,
        test_tool_use,
    },
    types::{TestResult, TestStatus},
};

#[test]
fn test_basic_qa_passes() {
    let result = test_basic_qa();
    assert_eq!(result.status, TestStatus::Pass, "test_basic_qa should pass");
}

#[test]
fn test_tool_use_passes() {
    let result = test_tool_use();
    assert_eq!(result.status, TestStatus::Pass, "test_tool_use should pass");
}

#[test]
fn test_multi_turn_passes() {
    let result = test_multi_turn();
    assert_eq!(
        result.status,
        TestStatus::Pass,
        "test_multi_turn should pass"
    );
}

#[test]
fn test_error_recovery_passes() {
    let result = test_error_recovery();
    assert_eq!(
        result.status,
        TestStatus::Pass,
        "test_error_recovery should pass"
    );
}

#[test]
fn test_guardian_enforcement_passes() {
    let result = test_guardian_enforcement();
    assert_eq!(
        result.status,
        TestStatus::Pass,
        "test_guardian_enforcement should pass"
    );
}

#[test]
fn test_rate_limit_simulation_passes() {
    let result = test_rate_limit_simulation();
    assert_eq!(
        result.status,
        TestStatus::Pass,
        "test_rate_limit_simulation should pass"
    );
}

#[test]
fn test_benchmark_performance_passes() {
    let result = benchmark_performance();
    assert_eq!(
        result.status,
        TestStatus::Pass,
        "benchmark_performance should pass"
    );
}

#[test]
fn test_junit_xml_output() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output_path = temp_dir.path().join("test-results.xml");

    let results = vec![
        TestResult::pass("test_a", 100, None),
        TestResult::fail("test_b", 200, "expected X got Y"),
        TestResult::skip("test_c"),
    ];

    write_junit_xml(&results, &output_path).unwrap();

    let xml_content = std::fs::read_to_string(&output_path).unwrap();

    // Verify XML structure
    assert!(xml_content.contains("<?xml version=\"1.0\""));
    assert!(xml_content.contains("name=\"synthia-e2e\""));
    assert!(xml_content.contains("tests=\"3\""));
    assert!(xml_content.contains("failures=\"1\""));
    assert!(xml_content.contains("skipped=\"1\""));
    assert!(xml_content.contains("name=\"test_a\""));
    assert!(xml_content.contains("name=\"test_b\""));
    assert!(xml_content.contains("name=\"test_c\""));
    assert!(xml_content.contains("<failure message=\"expected X got Y\" />"));
    assert!(xml_content.contains("<skipped />"));
    assert!(xml_content.contains("</testsuite>"));
}

#[test]
fn test_test_result_helpers() {
    let pass = TestResult::pass("test", 50, Some("all good".to_string()));
    assert_eq!(pass.status, TestStatus::Pass);
    assert_eq!(pass.duration_ms, 50);
    assert_eq!(pass.message, Some("all good".to_string()));

    let fail = TestResult::fail("test", 30, "something broke");
    assert_eq!(fail.status, TestStatus::Fail);
    assert_eq!(fail.failure_message, Some("something broke".to_string()));

    let skip = TestResult::skip("test");
    assert_eq!(skip.status, TestStatus::Skip);

    let run_pass = TestResult::run("test", || Ok(()));
    assert_eq!(run_pass.status, TestStatus::Pass);

    let run_fail = TestResult::run("test", || anyhow::bail!("oops"));
    assert_eq!(run_fail.status, TestStatus::Fail);
    assert!(run_fail.failure_message.unwrap().contains("oops"));
}

#[test]
fn test_xml_escaping() {
    assert_eq!(escape_xml("a & b"), "a &amp; b");
    assert_eq!(escape_xml("<tag>"), "&lt;tag&gt;");
    assert_eq!(escape_xml("\"quoted\""), "&quot;quoted&quot;");
}
