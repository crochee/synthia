//! synthia-e2e: End-to-end test scenarios for Synthia agent
//!
//! Provides test runners, mock LLM server, and scenario definitions
//! for validating agent behavior including Q&A, tool use, multi-turn
//! conversations, error recovery, guardian enforcement, and benchmarks.

pub mod fixtures;
pub mod mock_server;
pub mod runner;
pub mod scenarios;
pub mod utils;

// Re-export commonly used types
// Re-export fixtures
pub use fixtures::{agents, configs, skills};
pub use mock_server::{
    MockError,
    MockLlmServer,
    MockToolCall,
    ScriptedResponse,
};
pub use runner::{
    TestResult,
    TestStatus,
    benchmark_performance,
    run_all_tests,
    run_and_report,
    test_basic_qa,
    test_error_recovery,
    test_guardian_enforcement,
    test_multi_turn,
    test_rate_limit_simulation,
    test_tool_use,
    write_junit_xml,
};
// Re-export utils
pub use utils::{assert, mock_provider};
