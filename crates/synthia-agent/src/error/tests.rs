//! Unit tests for the `error` module family.
//!
//! The original 30 tests lived at the bottom of
//! `error/mod.rs`; they're hoisted into this sibling
//! file so the test bodies don't bloat the module
//! re-export shim.
//!
//! Coverage map:
//!
//! - Constructor methods: `test_tool_error`,
//!   `test_tool_error_convenience`, `test_session_error`,
//!   `test_context_error`, `test_config_error`,
//!   `test_validation_error`, `test_timeout_error`,
//!   `test_internal_error`, `test_database_error`,
//!   `test_file_conflict_error`, `test_rate_limited_error`,
//!   `test_context_window_exceeded_error`,
//!   `test_pool_closed_error`,
//!   `test_tool_approval_required_error`,
//!   `test_invalid_operation_error`,
//!   `test_guardian_denied_error`.
//! - `From` impls: `test_from_string`, `test_from_str`,
//!   `test_from_provider_error`, `test_from_join_error`.
//! - Display formatting: `test_error_message_formatting`.
//! - `ProviderErrorContext`: `test_provider_with_context`,
//!   `test_provider_with_context_non_retryable`,
//!   `test_provider_error_context_display_no_status`,
//!   `test_provider_error_context_source`,
//!   `test_from_provider_error_context`.
//! - `is_retryable` / `is_*` predicates:
//!   `test_cancelled_error`, `test_is_retryable_for_cancelled`,
//!   `test_is_retryable_for_rate_limited`,
//!   `test_is_retryable_for_timeout`,
//!   `test_is_retryable_for_non_retryable_errors`,
//!   `test_is_not_timeout`, `test_is_not_rate_limited`,
//!   `test_is_not_context_window_exceeded`.

use std::error::Error;

use synthia_provider::ProviderError;

use super::core::{AgentError, ProviderErrorContext};

#[test]
fn test_tool_error() {
    let error = AgentError::tool("my_tool", "failed");
    match error {
        AgentError::ToolError { tool, message } => {
            assert_eq!(tool, "my_tool");
            assert_eq!(message, "failed");
        }
        _ => panic!("Expected ToolError"),
    }
}

#[test]
fn test_tool_error_convenience() {
    let error = AgentError::tool_error("something failed");
    match error {
        AgentError::ToolError { tool, message } => {
            assert_eq!(tool, "unknown");
            assert_eq!(message, "something failed");
        }
        _ => panic!("Expected ToolError"),
    }
}

#[test]
fn test_from_string() {
    let error = AgentError::from("test error");
    match error {
        AgentError::InternalError(msg) => assert_eq!(msg, "test error"),
        _ => panic!("Expected InternalError"),
    }
}

#[test]
fn test_from_provider_error() {
    let provider_error = ProviderError::api("API error");
    let agent_error = AgentError::from(provider_error);
    assert!(matches!(agent_error, AgentError::Provider(_)));
}

#[test]
fn test_error_message_formatting() {
    let error = AgentError::tool("tool", "message");
    assert!(error.to_string().contains("tool"));
    assert!(error.to_string().contains("message"));
}

#[test]
fn test_session_error() {
    let error = AgentError::session("session not found");
    assert!(matches!(error, AgentError::SessionError(_)));
    assert!(error.to_string().contains("session not found"));
}

#[test]
fn test_context_error() {
    let error = AgentError::context("context overflow");
    assert!(matches!(error, AgentError::ContextError(_)));
    assert!(error.to_string().contains("context overflow"));
}

#[test]
fn test_config_error() {
    let error = AgentError::config("missing field");
    assert!(matches!(error, AgentError::ConfigError(_)));
    assert!(error.to_string().contains("missing field"));
}

#[test]
fn test_validation_error() {
    let error = AgentError::validation("invalid input");
    assert!(matches!(error, AgentError::ValidationError(_)));
    assert!(error.to_string().contains("invalid input"));
}

#[test]
fn test_timeout_error() {
    let error = AgentError::timeout("operation timed out");
    assert!(error.is_timeout());
    assert!(error.to_string().contains("operation timed out"));
}

#[test]
fn test_internal_error() {
    let error = AgentError::internal("unexpected");
    assert!(matches!(error, AgentError::InternalError(_)));
    assert!(error.to_string().contains("unexpected"));
}

#[test]
fn test_database_error() {
    let error = AgentError::database("connection failed");
    assert!(matches!(error, AgentError::DatabaseError(_)));
    assert!(error.to_string().contains("connection failed"));
}

#[test]
fn test_file_conflict_error() {
    let error = AgentError::file_conflict("/path/to/file");
    assert!(matches!(error, AgentError::FileConflict { .. }));
    assert!(error.to_string().contains("/path/to/file"));
}

#[test]
fn test_rate_limited_error() {
    let error = AgentError::rate_limited(Some(60));
    assert!(error.is_rate_limited());
    assert!(error.to_string().contains("60"));
}

#[test]
fn test_context_window_exceeded_error() {
    let error = AgentError::context_window_exceeded(10000, 8000);
    assert!(error.is_context_window_exceeded());
    assert!(error.to_string().contains("10000"));
    assert!(error.to_string().contains("8000"));
}

#[test]
fn test_from_join_error() {
    // Note: We can't easily create a JoinError, but we can test the conversion
    // This is a compile-time check that the conversion exists
    fn _check_conversion(e: tokio::task::JoinError) -> AgentError {
        AgentError::from(e)
    }
}

#[test]
fn test_from_str() {
    let error: AgentError = "test string error".into();
    match error {
        AgentError::InternalError(msg) => {
            assert_eq!(msg, "test string error")
        }
        _ => panic!("Expected InternalError"),
    }
}

#[test]
fn test_tool_approval_required_error() {
    let error = AgentError::ToolApprovalRequired("read_file".to_string());
    assert!(error.to_string().contains("Tool approval required"));
    assert!(error.to_string().contains("read_file"));
}

#[test]
fn test_invalid_operation_error() {
    let error = AgentError::InvalidOperation("cannot proceed".to_string());
    assert!(error.to_string().contains("Invalid operation"));
    assert!(error.to_string().contains("cannot proceed"));
}

#[test]
fn test_guardian_denied_error() {
    let error = AgentError::GuardianDenied("action not allowed".to_string());
    assert!(error.to_string().contains("Guardian denied"));
    assert!(error.to_string().contains("action not allowed"));
}

#[test]
fn test_cancelled_error() {
    let error = AgentError::Cancelled;
    assert!(error.to_string().contains("Operation cancelled"));
    assert!(error.is_retryable());
}

#[test]
fn test_pool_closed_error() {
    let error = AgentError::pool_closed("connection pool terminated");
    match error {
        AgentError::PoolClosed(ref msg) => {
            assert_eq!(msg, "connection pool terminated")
        }
        _ => panic!("Expected PoolClosed"),
    }
    assert!(error.to_string().contains("Pool closed"));
}

#[test]
fn test_provider_with_context() {
    let ctx = ProviderErrorContext {
        error: ProviderError::RateLimitError("rate limited".to_string()),
        status_code: Some(429),
        retryable: true,
    };
    let error = AgentError::provider_with_context(ctx);
    assert!(matches!(error, AgentError::ProviderWithContext(_)));
    assert!(error.is_retryable());
}

#[test]
fn test_provider_with_context_non_retryable() {
    let ctx = ProviderErrorContext {
        error: ProviderError::api("unauthorized"),
        status_code: Some(401),
        retryable: false,
    };
    let error = AgentError::provider_with_context(ctx);
    assert!(!error.is_retryable());
    assert!(error.to_string().contains("[status=401]"));
    assert!(error.to_string().contains("[non-retryable]"));
}

#[test]
fn test_provider_error_context_display_no_status() {
    let ctx = ProviderErrorContext {
        error: ProviderError::api("error"),
        status_code: None,
        retryable: true,
    };
    let display = ctx.to_string();
    assert!(!display.contains("[status="));
    assert!(display.contains("error"));
}

#[test]
fn test_provider_error_context_source() {
    let ctx = ProviderErrorContext {
        error: ProviderError::Timeout,
        status_code: Some(408),
        retryable: true,
    };
    let source = ctx.source();
    assert!(source.is_some());
}

#[test]
fn test_from_provider_error_context() {
    let ctx = ProviderErrorContext {
        error: ProviderError::api("api error"),
        status_code: Some(500),
        retryable: false,
    };
    let error: AgentError = ctx.into();
    assert!(matches!(error, AgentError::ProviderWithContext(_)));
}

#[test]
fn test_is_retryable_for_cancelled() {
    let error = AgentError::Cancelled;
    assert!(error.is_retryable());
}

#[test]
fn test_is_retryable_for_rate_limited() {
    let error = AgentError::rate_limited(None);
    assert!(error.is_retryable());
}

#[test]
fn test_is_retryable_for_timeout() {
    let error = AgentError::timeout("too long");
    assert!(error.is_retryable());
}

#[test]
fn test_is_retryable_for_non_retryable_errors() {
    assert!(!AgentError::tool("t", "m").is_retryable());
    assert!(!AgentError::config("c").is_retryable());
    assert!(!AgentError::validation("v").is_retryable());
    assert!(!AgentError::context_window_exceeded(100, 50).is_retryable());
}

#[test]
fn test_is_not_timeout() {
    let error = AgentError::tool("t", "m");
    assert!(!error.is_timeout());
}

#[test]
fn test_is_not_rate_limited() {
    let error = AgentError::tool("t", "m");
    assert!(!error.is_rate_limited());
}

#[test]
fn test_is_not_context_window_exceeded() {
    let error = AgentError::tool("t", "m");
    assert!(!error.is_context_window_exceeded());
}
