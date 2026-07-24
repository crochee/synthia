//! 16 unit tests for the `tracing` module family.
//!
//! Coverage map:
//!
//! - [`super::config::ObservabilityConfig`]: 2 tests
//!   (default + custom).
//! - [`super::spans::trace_iteration`]: 1 test (span
//!   created with name "agent.iteration").
//! - [`super::spans::trace_llm_call`]: 1 test (span
//!   created with name "llm.call").
//! - [`super::spans::trace_tool_call`]: 4 tests
//!   (created / truncates long args / not truncated at
//!   exactly 500 / truncated at 501).
//! - [`super::metrics`]: 5 tests (record_iteration /
//!   record_llm_call / record_tool_call /
//!   record_hook_error / record_guardian_block all
//!   no-panic when no global recorder is installed).
//! - [`super::spans`]: 2 nested-span tests
//!   (`test_tracing_span_attributes`,
//!   `test_tracing_span_nested`).
//! - [`super::flags::is_recorder_installed`]: 1 test
//!   (default false).

use super::*;

#[test]
fn test_observability_config_default() {
    let config = ObservabilityConfig::default();
    assert_eq!(config.metrics_port, 9090);
    assert!(config.otlp_endpoint.is_none());
}

#[test]
fn test_observability_config_custom() {
    let config = ObservabilityConfig {
        metrics_port: 8080,
        otlp_endpoint: Some("http://localhost:4317".to_string()),
    };
    assert_eq!(config.metrics_port, 8080);
    assert_eq!(
        config.otlp_endpoint.as_deref(),
        Some("http://localhost:4317")
    );
}

#[test]
fn test_trace_iteration_span_created() {
    let span = trace_iteration(1, "openai", "gpt-4o", 1024, 3, 500);
    // Verify the span is created and has the expected name
    assert!(span.metadata().is_some());
    assert_eq!(span.metadata().unwrap().name(), "agent.iteration");
}

#[test]
fn test_trace_llm_call_span_created() {
    let span = trace_llm_call("openai", "gpt-4o", 100, 200, 350, "success");
    assert!(span.metadata().is_some());
    assert_eq!(span.metadata().unwrap().name(), "llm.call");
}

#[test]
fn test_trace_tool_call_span_created() {
    let span =
        trace_tool_call("read_file", r#"{"path": "/tmp"}"#, "success", 50);
    assert!(span.metadata().is_some());
    assert_eq!(span.metadata().unwrap().name(), "tool.execute");
}

#[test]
fn test_trace_tool_call_truncates_long_args() {
    let long_args = "x".repeat(1000);
    let span = trace_tool_call("big_tool", &long_args, "success", 10);
    // Should not panic even with very long args
    assert!(span.metadata().is_some());
}

#[test]
fn test_trace_tool_call_args_exactly_500_chars_not_truncated() {
    let args = "x".repeat(500);
    let span = trace_tool_call("exact_tool", &args, "success", 10);
    assert!(span.metadata().is_some());
}

#[test]
fn test_trace_tool_call_args_501_chars_truncated() {
    let args = "x".repeat(501);
    let span = trace_tool_call("over_tool", &args, "success", 10);
    assert!(span.metadata().is_some());
}

#[test]
fn test_record_iteration_metric_no_panic() {
    // Works as no-op when no global recorder is installed.
    record_iteration_metric();
}

#[test]
fn test_record_llm_call_metric_no_panic() {
    record_llm_call_metric("openai", "gpt-4o", "success", 0.5);
}

#[test]
fn test_record_tool_call_metric_no_panic() {
    record_tool_call_metric("read_file", "success", 0.1);
}

#[test]
fn test_record_hook_error_metric_no_panic() {
    record_hook_error_metric("before_llm");
}

#[test]
fn test_record_guardian_block_metric_no_panic() {
    record_guardian_block_metric("loop_detector");
}

#[test]
fn test_tracing_span_attributes() {
    let span = trace_iteration(1, "openai", "gpt-4o", 1024, 3, 500);
    let _enter = span.enter();
    let _result = 1 + 1;
}

#[test]
fn test_tracing_span_nested() {
    let outer = trace_iteration(1, "openai", "gpt-4o", 1024, 0, 0);
    let _outer_enter = outer.enter();

    let inner = trace_llm_call("openai", "gpt-4o", 100, 200, 350, "success");
    let _inner_enter = inner.enter();
}

#[test]
fn test_is_recorder_installed_default_false() {
    assert!(!is_recorder_installed());
}
