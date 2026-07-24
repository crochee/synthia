//! Result / status data carriers for the e2e runner.
//!
//! Two types live here, kept together because the
//! [`TestStatus`] enum is only ever used inside
//! [`TestResult::status`]:
//!
//! - [`TestResult`]: the value each test scenario
//!   returns. Holds the test name, status, duration
//!   in milliseconds, and an optional
//!   success-message / failure-message.
//! - [`TestStatus`]: `Pass` / `Fail` / `Skip`. The
//!   `Display` impl renders it as the lowercase
//!   string the JUnit XML emitter and the
//!   `tracing::info!` calls in [`super::run`] both
//!   expect.
//!
//! The two constructors ([`TestResult::run`] and
//! the convenience [`TestResult::pass`] /
//! [`TestResult::fail`] / [`TestResult::skip`]
//! helpers) are also kept here — they don't touch any
//! of the other submodules, so keeping them with the
//! data they construct lets a reader understand
//! "what is a `TestResult`" without scrolling.

use std::time::Instant;

use anyhow::Result;

/// The result of a single e2e test scenario.
#[derive(Clone, Debug)]
pub struct TestResult {
    /// Test name — also the value `tracing::info!`
    /// surfaces and the `name` attribute the JUnit
    /// XML emitter uses.
    pub name: String,
    /// Pass / Fail / Skip. See [`TestStatus`].
    pub status: TestStatus,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// Optional human-readable success message.
    /// Always `None` for `Fail` / `Skip` results.
    pub message: Option<String>,
    /// Optional human-readable failure message.
    /// Always `Some` for `Fail` results, `None`
    /// otherwise.
    pub failure_message: Option<String>,
}

impl TestResult {
    /// Build a `Pass` result.
    pub fn pass(
        name: impl Into<String>,
        duration_ms: u64,
        message: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status: TestStatus::Pass,
            duration_ms,
            message,
            failure_message: None,
        }
    }

    /// Build a `Fail` result.
    pub fn fail(
        name: impl Into<String>,
        duration_ms: u64,
        failure_message: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status: TestStatus::Fail,
            duration_ms,
            message: None,
            failure_message: Some(failure_message.into()),
        }
    }

    /// Build a `Skip` result. `duration_ms` is
    /// hard-coded to 0 — skipping is a free
    /// operation and shouldn't be billed to any
    /// timer.
    pub fn skip(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: TestStatus::Skip,
            duration_ms: 0,
            message: None,
            failure_message: None,
        }
    }

    /// Time a `FnOnce() -> Result<()>` and convert
    /// the result into a `Pass` / `Fail`
    /// [`TestResult`]. The most common entry point
    /// for [`super::scenarios`].
    pub fn run<F>(name: impl Into<String>, f: F) -> Self
    where
        F: FnOnce() -> Result<()>,
    {
        let name: String = name.into();
        let start = Instant::now();
        match f() {
            Ok(()) => {
                TestResult::pass(name, start.elapsed().as_millis() as u64, None)
            }
            Err(e) => TestResult::fail(
                name,
                start.elapsed().as_millis() as u64,
                e.to_string(),
            ),
        }
    }
}

/// Pass / Fail / Skip verdict for a [`TestResult`].
#[derive(Clone, Debug, PartialEq)]
pub enum TestStatus {
    /// The scenario completed without error.
    Pass,
    /// The scenario returned `Err`.
    Fail,
    /// The scenario was not executed.
    Skip,
}

impl std::fmt::Display for TestStatus {
    /// Lowercase rendering — the JUnit XML emitter
    /// uses this for the `status="..."` attribute
    /// and the `tracing::info!` calls in
    /// [`super::run`] use it for the prefix label.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestStatus::Pass => write!(f, "pass"),
            TestStatus::Fail => write!(f, "fail"),
            TestStatus::Skip => write!(f, "skip"),
        }
    }
}
