//! Unit tests for the `audit` module family.
//!
//! Coverage map (14 tests):
//!
//! - Directory creation: 1 test
//!   ([`test_audit_logger_creates_directory`]).
//! - `AuditEntry`: 4 tests
//!   ([`test_audit_entry_creation`],
//!   [`test_audit_entry_serialization`],
//!   [`test_audit_entry_timestamp_format`]).
//! - Display: 2 tests
//!   ([`test_audit_event_type_display`],
//!   [`test_audit_severity_display`]).
//! - `AuditLogger` core: 4 tests
//!   ([`test_audit_logger_flush`],
//!   [`test_audit_logger_auto_flush`],
//!   [`test_multiple_entries_separate_lines`],
//!   [`test_flush_empty_buffer`]).
//! - Typed helper methods: 5 tests
//!   ([`test_log_permission_denied`],
//!   [`test_log_input_blocked`],
//!   [`test_log_output_blocked`],
//!   [`test_log_credential_redacted`],
//!   [`test_log_loop_detected`],
//!   [`test_log_circuit_breaker`]).
//! - `FileAuditWriter`: 1 test
//!   ([`test_file_audit_writer`]).

use tempfile::TempDir;

use super::*;

/// Test that creating an AuditLogger creates the .synthia directory.
#[test]
fn test_audit_logger_creates_directory() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path();

    let _logger = AuditLogger::new(workspace_root).unwrap();

    let synthia_dir = workspace_root.join(".synthia");
    assert!(synthia_dir.exists());
    assert!(synthia_dir.is_dir());
}

/// Test that AuditEntry is created with correct fields.
#[test]
fn test_audit_entry_creation() {
    let entry = AuditEntry::new(
        AuditEventType::PermissionGranted,
        AuditSeverity::Info,
        serde_json::json!({"tool_name": "read_file"}),
        "session-1".to_string(),
    );

    assert_eq!(entry.event_type, AuditEventType::PermissionGranted);
    assert_eq!(entry.severity, AuditSeverity::Info);
    assert_eq!(entry.session_id, "session-1");
    assert!(!entry.timestamp.is_empty());
}

/// Test that AuditEventType displays correctly.
#[test]
fn test_audit_event_type_display() {
    assert_eq!(
        AuditEventType::PermissionGranted.to_string(),
        "permission_granted"
    );
    assert_eq!(
        AuditEventType::PermissionDenied.to_string(),
        "permission_denied"
    );
    assert_eq!(
        AuditEventType::CircuitBreakerTriggered.to_string(),
        "circuit_breaker_triggered"
    );
}

/// Test that AuditSeverity displays correctly.
#[test]
fn test_audit_severity_display() {
    assert_eq!(AuditSeverity::Info.to_string(), "info");
    assert_eq!(AuditSeverity::Warning.to_string(), "warning");
    assert_eq!(AuditSeverity::Error.to_string(), "error");
    assert_eq!(AuditSeverity::Critical.to_string(), "critical");
}

/// Test that AuditEntry serializes to valid JSON.
#[test]
fn test_audit_entry_serialization() {
    let entry = AuditEntry::new(
        AuditEventType::LoopDetected,
        AuditSeverity::Warning,
        serde_json::json!({"loop_type": "tool", "action_taken": "blocked"}),
        "session-2".to_string(),
    );

    let json = serde_json::to_string(&entry).unwrap();
    assert!(json.contains("loop_detected"));
    assert!(json.contains("warning"));
    assert!(json.contains("session-2"));
    assert!(json.contains("tool"));
}

/// Test that audit log entries are flushed to file.
#[test]
fn test_audit_logger_flush() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("audit.log");

    let mut logger = AuditLogger::with_path(log_path.clone());

    logger.log_permission_granted(
        "test-session",
        "read_file",
        "Allowed by policy",
    );
    logger.flush().unwrap();

    assert!(log_path.exists());

    let content = std::fs::read_to_string(&log_path).unwrap();
    assert!(content.contains("permission_granted"));
    assert!(content.contains("read_file"));
}

/// Test that buffer auto-flushes when full.
#[test]
fn test_audit_logger_auto_flush() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("audit.log");

    let mut logger =
        AuditLogger::with_path(log_path.clone()).with_max_buffer_size(2);

    logger.log_permission_granted("s1", "tool1", "reason1");
    logger.log_permission_granted("s2", "tool2", "reason2");
    // Buffer is now full (size 2), next log should trigger flush

    logger.log_permission_granted("s3", "tool3", "reason3");

    // After flushing, the first two entries should be in the file
    let content = std::fs::read_to_string(&log_path).unwrap();
    assert!(content.contains("tool1"));
    assert!(content.contains("tool2"));
}

/// Test helper method: log_permission_denied.
#[test]
fn test_log_permission_denied() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("audit.log");

    let mut logger = AuditLogger::with_path(log_path.clone());
    logger.log_permission_denied("s1", "write_file", "Not in allowlist");
    logger.flush().unwrap();

    let content = std::fs::read_to_string(&log_path).unwrap();
    assert!(content.contains("permission_denied"));
    assert!(content.contains("write_file"));
    assert!(content.contains("warning"));
}

/// Test helper method: log_input_blocked.
#[test]
fn test_log_input_blocked() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("audit.log");

    let mut logger = AuditLogger::with_path(log_path.clone());
    logger.log_input_blocked("s1", "DROP TABLE");
    logger.flush().unwrap();

    let content = std::fs::read_to_string(&log_path).unwrap();
    assert!(content.contains("input_blocked"));
    assert!(content.contains("DROP TABLE"));
}

/// Test helper method: log_output_blocked.
#[test]
fn test_log_output_blocked() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("audit.log");

    let mut logger = AuditLogger::with_path(log_path.clone());
    logger.log_output_blocked("s1", "<script>");
    logger.flush().unwrap();

    let content = std::fs::read_to_string(&log_path).unwrap();
    assert!(content.contains("output_blocked"));
}

/// Test helper method: log_credential_redacted.
#[test]
fn test_log_credential_redacted() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("audit.log");

    let mut logger = AuditLogger::with_path(log_path.clone());
    logger.log_credential_redacted("s1", "api_key");
    logger.flush().unwrap();

    let content = std::fs::read_to_string(&log_path).unwrap();
    assert!(content.contains("credential_redacted"));
    assert!(content.contains("api_key"));
}

/// Test helper method: log_loop_detected.
#[test]
fn test_log_loop_detected() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("audit.log");

    let mut logger = AuditLogger::with_path(log_path.clone());
    logger.log_loop_detected("s1", "tool_loop", "blocked_and_warned");
    logger.flush().unwrap();

    let content = std::fs::read_to_string(&log_path).unwrap();
    assert!(content.contains("loop_detected"));
    assert!(content.contains("tool_loop"));
    assert!(content.contains("blocked_and_warned"));
}

/// Test helper method: log_circuit_breaker.
#[test]
fn test_log_circuit_breaker() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("audit.log");

    let mut logger = AuditLogger::with_path(log_path.clone());
    logger.log_circuit_breaker("s1", "openai", "open");
    logger.flush().unwrap();

    let content = std::fs::read_to_string(&log_path).unwrap();
    assert!(content.contains("circuit_breaker_triggered"));
    assert!(content.contains("openai"));
    assert!(content.contains("open"));
}

/// Test AuditEntry ISO 8601 timestamp format.
#[test]
fn test_audit_entry_timestamp_format() {
    let entry = AuditEntry::new(
        AuditEventType::AgentStarted,
        AuditSeverity::Info,
        serde_json::json!({}),
        "s1".to_string(),
    );

    // ISO 8601 / RFC 3339 format should contain 'T' and timezone info
    assert!(entry.timestamp.contains('T'));
}

/// Test that multiple entries are serialized as separate lines.
#[test]
fn test_multiple_entries_separate_lines() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("audit.log");

    let mut logger = AuditLogger::with_path(log_path.clone());

    logger.log_permission_granted("s1", "tool_a", "ok");
    logger.log_permission_denied("s1", "tool_b", "blocked");
    logger.flush().unwrap();

    let content = std::fs::read_to_string(&log_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);
}

/// Test flushing empty buffer is a no-op.
#[test]
fn test_flush_empty_buffer() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("audit.log");

    let mut logger = AuditLogger::with_path(log_path.clone());
    // Flush without logging anything
    logger.flush().unwrap();

    // File should not exist since nothing was written
    assert!(!log_path.exists());
}

/// Test FileAuditWriter (inherent method, no trait).
#[tokio::test]
async fn test_file_audit_writer() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("audit.log");

    let mut writer = FileAuditWriter::new(log_path.clone());
    let entry = AuditEntry::new(
        AuditEventType::HookError,
        AuditSeverity::Error,
        serde_json::json!({"hook": "test_hook"}),
        "s1".to_string(),
    );

    writer.write(&entry).await.unwrap();

    let content = std::fs::read_to_string(&log_path).unwrap();
    assert!(content.contains("hook_error"));
    assert!(content.contains("test_hook"));
}
