//! [`synthia_test_support`] — deterministic fakes
//! for other crates' test suites.
//!
//! [`ReplayProvider`] replays recorded model-call scripts (built from
//! `Vec<Vec<StreamChunk>>` or a session `events.jsonl` fixture) with
//! no network, no credentials, and a completeness check at teardown;
//! `fake_tool` and `run_stream` do the same job for tool dispatch.
//! Enable the facade's `test-support` feature from `[dev-dependencies]`
//! only.

pub use synthia_test_support::*;
