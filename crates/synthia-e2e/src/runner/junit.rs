//! JUnit XML emitter for CI/CD integration.
//!
//! Two functions live here:
//!
//! - [`write_junit_xml`]: the public entry point.
//!   Takes a `&[TestResult]` slice + a destination
//!   path, and writes a JUnit-format XML file.
//!   Returns `Err` (wrapped in
//!   `anyhow::Context`) if the write fails.
//! - [`escape_xml`]: the private helper that
//!   escapes the 5 XML metacharacters
//!   (`&` / `<` / `>` / `"` / `'`) in test names
//!   and failure messages. Without it, a test name
//!   like `<smoke>` would corrupt the output XML.
//!
//! Kept separate from [`super::run`] so the XML
//! rendering can be unit-tested directly (see
//! [`super::super::tests::test_junit_xml_output`])
//! without dragging the whole test-runner
//! orchestration into scope.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;

use super::types::{TestResult, TestStatus};

/// Write the test results to a JUnit XML file at
/// `output_path`.
///
/// The emitted document has the shape CI systems
/// like Jenkins / GitLab CI / CircleCI expect:
///
/// ```xml
/// <?xml version="1.0" encoding="UTF-8"?>
/// <testsuite name="synthia-e2e" tests="N" failures="F"
///            skipped="S" timestamp="2026-06-19T...">
///   <testcase name="..." time="0.123" status="pass" />
///   <testcase name="..." time="0.045" status="fail">
///     <failure message="..." />
///   </testcase>
///   <testcase name="..." time="0" status="skip">
///     <skipped />
///   </testcase>
/// </testsuite>
/// ```
pub fn write_junit_xml(
    results: &[TestResult],
    output_path: &Path,
) -> Result<()> {
    let total = results.len();
    let failures = results
        .iter()
        .filter(|r| r.status == TestStatus::Fail)
        .count();
    let skipped = results
        .iter()
        .filter(|r| r.status == TestStatus::Skip)
        .count();
    let _passed = total - failures - skipped;

    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ");

    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str(&format!(
        "<testsuite name=\"synthia-e2e\" tests=\"{}\" failures=\"{}\" skipped=\"{}\" timestamp=\"{}\">\n",
        total, failures, skipped, timestamp
    ));

    for result in results {
        let duration_sec = result.duration_ms as f64 / 1000.0;

        xml.push_str(&format!(
            "  <testcase name=\"{}\" time=\"{:.3}\" status=\"{}\"",
            escape_xml(&result.name),
            duration_sec,
            result.status
        ));

        match &result.status {
            TestStatus::Pass => {
                xml.push_str(" />\n");
            }
            TestStatus::Fail => {
                xml.push_str(">\n");
                if let Some(ref msg) = result.failure_message {
                    xml.push_str(&format!(
                        "    <failure message=\"{}\" />\n",
                        escape_xml(msg)
                    ));
                }
                xml.push_str("  </testcase>\n");
            }
            TestStatus::Skip => {
                xml.push_str(">\n");
                xml.push_str("    <skipped />\n");
                xml.push_str("  </testcase>\n");
            }
        }
    }

    xml.push_str("</testsuite>\n");

    std::fs::write(output_path, xml).with_context(|| {
        format!("Failed to write JUnit XML to {}", output_path.display())
    })
}

/// Escape the 5 XML metacharacters in a string.
/// Applied to every test name and failure message
/// before they're interpolated into the JUnit XML.
///
/// `pub(super)` so the [`super::tests`] submodule
/// can unit-test the escaping directly.
pub(super) fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
