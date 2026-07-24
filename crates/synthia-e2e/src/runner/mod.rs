//! E2E test runner family.
//!
//! The original 764-line `runner.rs` was split into
//! focused submodules by responsibility:
//!
//! - [`types`]: the [`types::TestResult`] data
//!   carrier + the [`types::TestStatus`] enum + the
//!   [`types::TestResult::pass`] /
//!   [`types::TestResult::fail`] /
//!   [`types::TestResult::skip`] /
//!   [`types::TestResult::run`] constructors.
//! - [`scenarios`]: the 7 public scenario functions
//!   ([`scenarios::test_basic_qa`],
//!   [`scenarios::test_tool_use`],
//!   [`scenarios::test_multi_turn`],
//!   [`scenarios::test_error_recovery`],
//!   [`scenarios::test_guardian_enforcement`],
//!   [`scenarios::test_rate_limit_simulation`],
//!   [`scenarios::benchmark_performance`]).
//! - [`junit`]: the JUnit XML emitter
//!   ([`junit::write_junit_xml`]) + the private
//!   `escape_xml` helper.
//! - [`run`]: the top-level orchestration
//!   ([`run::run_all_tests`] / [`run::run_and_report`]).
//!
//! The 8+ unit tests live in [`tests`].

mod junit;
mod run;
mod scenarios;
mod types;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use junit::write_junit_xml;
pub use run::{run_all_tests, run_and_report};
pub use scenarios::{
    benchmark_performance,
    test_basic_qa,
    test_error_recovery,
    test_guardian_enforcement,
    test_multi_turn,
    test_rate_limit_simulation,
    test_tool_use,
};
pub use types::{TestResult, TestStatus};
