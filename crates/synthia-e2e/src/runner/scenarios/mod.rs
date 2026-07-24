//! E2E test scenarios.
//!
//! Seven public entry points, one per scenario.
//! Each is a `TestResult::run(name, || { ... })` call
//! that drives a [`MockLlmServer`] through one
//! capability and verifies the response. The
//! scenarios are intentionally thin — they don't
//! know about the JUnit XML emitter, the
//! `tracing::info!` calls, or the
//! pass/fail/skip counter; that orchestration lives
//! in [`super::run`].
//!
//! ## What each scenario covers
//!
//! - [`test_basic_qa`]: simple text Q&A. Verifies
//!   `Paris` shows up in the response and that the
//!   mock didn't return a tool call.
//! - [`test_tool_use`]: writes a temp file, asks
//!   the mock to call `read_file` against it,
//!   verifies the tool call path matches the file
//!   path.
//! - [`test_multi_turn`]: queues two responses and
//!   verifies the second turn resolves "it" to
//!   `config.toml` (the entity the first turn
//!   introduced).
//! - [`test_error_recovery`]: scripts an
//!   "apology / could not be found" response and
//!   verifies it addresses the file-not-found
//!   error.
//! - [`test_guardian_enforcement`]: smoke test —
//!   verifies a clean input is accepted (the
//!   comment notes that the legacy guardian
//!   enforcement was removed; the test stays as a
//!   canary for input handling).
//! - [`test_rate_limit_simulation`]: queues three
//!   responses, sets a rate limit after 2 calls,
//!   verifies the 3rd call returns `429`.
//! - [`benchmark_performance`]: 100-iteration Q&A
//!   loop that reports `iterations/s` and
//!   `tool_calls/s` in the success message.

mod benchmark;
mod conversation;
mod limits;
mod qa;
mod tool_use;

pub use benchmark::benchmark_performance;
pub use conversation::{test_error_recovery, test_multi_turn};
pub use limits::{test_guardian_enforcement, test_rate_limit_simulation};
pub use qa::test_basic_qa;
pub use tool_use::test_tool_use;
