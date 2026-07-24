//! Top-level test-runner orchestration.
//!
//! Two public entry points:
//!
//! - [`run_all_tests`]: just runs every scenario in
//!   [`super::scenarios`] and returns the
//!   `Vec<TestResult>`. **No logging, no JUnit XML,
//!   no exit-code semantics** — the lightweight
//!   path used by callers that just want the raw
//!   results (tests, REPL `/e2e` command, etc).
//! - [`run_and_report`]: the heavy path. Runs every
//!   scenario, logs a per-test `tracing` line
//!   (with pass/fail/skip prefix), optionally writes
//!   a JUnit XML file, and returns
//!   `Err("N test(s) failed")` when at least one
//!   scenario failed. This is what the CLI's
//!   `e2e run` subcommand calls.
//!
//! The split keeps the "just give me the results"
//! call site clean — it doesn't have to opt out of
//! logging / JUnit by passing a bunch of `None`s.

use std::path::Path;

use anyhow::Result;

use super::{
    junit::write_junit_xml,
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

/// Run every scenario in [`super::scenarios`] in
/// order and return the `Vec<TestResult>`. No
/// logging, no JUnit XML — see [`run_and_report`]
/// for the orchestrating variant.
pub fn run_all_tests() -> Vec<TestResult> {
    vec![
        test_basic_qa(),
        test_tool_use(),
        test_multi_turn(),
        test_error_recovery(),
        test_guardian_enforcement(),
        test_rate_limit_simulation(),
        benchmark_performance(),
    ]
}

/// Run every scenario, log a per-test `tracing`
/// line, optionally emit a JUnit XML file at
/// `output_path`, and return the
/// `Vec<TestResult>` (or `Err` if any scenario
/// failed).
///
/// Exit semantics: returns `Err("N test(s) failed")`
/// when `failed > 0`, otherwise `Ok(results)`.
/// Callers that need an exit code can simply match
/// on this and propagate.
pub fn run_and_report(output_path: Option<&Path>) -> Result<Vec<TestResult>> {
    let results = run_all_tests();

    let passed = results
        .iter()
        .filter(|r| r.status == TestStatus::Pass)
        .count();
    let failed = results
        .iter()
        .filter(|r| r.status == TestStatus::Fail)
        .count();
    let skipped = results
        .iter()
        .filter(|r| r.status == TestStatus::Skip)
        .count();

    tracing::info!(
        "E2E test results: {} passed, {} failed, {} skipped ({} total)",
        passed,
        failed,
        skipped,
        results.len()
    );

    for result in &results {
        match result.status {
            TestStatus::Pass => {
                tracing::info!(
                    "  PASS  {} ({}ms)",
                    result.name,
                    result.duration_ms
                )
            }
            TestStatus::Fail => tracing::error!(
                "  FAIL  {} ({}ms): {}",
                result.name,
                result.duration_ms,
                result.failure_message.as_deref().unwrap_or("")
            ),
            TestStatus::Skip => tracing::warn!("  SKIP  {}", result.name),
        }
    }

    if let Some(path) = output_path {
        write_junit_xml(&results, path)?;
        tracing::info!("JUnit XML written to {}", path.display());
    }

    if failed > 0 {
        anyhow::bail!("{} test(s) failed", failed);
    }

    Ok(results)
}
