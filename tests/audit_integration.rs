use std::fs;

use synthia_agent::audit::{
    AuditEntry,
    AuditEventType,
    AuditLogger,
    AuditSeverity,
};
use tempfile::TempDir;

#[test]
fn test_audit_logger_lifecycle() {
    let temp_dir = TempDir::new().unwrap();
    let mut logger = AuditLogger::new(temp_dir.path()).unwrap();

    logger.log(AuditEntry::new(
        AuditEventType::AgentStarted,
        AuditSeverity::Info,
        serde_json::json!({"config": "test"}),
        "session-1".to_string(),
    ));

    logger.log(AuditEntry::new(
        AuditEventType::HookError,
        AuditSeverity::Error,
        serde_json::json!({"hook": "test_hook", "error": "panic"}),
        "session-1".to_string(),
    ));

    logger.log(AuditEntry::new(
        AuditEventType::AgentStopped,
        AuditSeverity::Info,
        serde_json::json!({"reason": "completed"}),
        "session-1".to_string(),
    ));

    logger.flush().unwrap();

    let audit_path = temp_dir.path().join(".synthia").join("audit.log");
    assert!(audit_path.exists(), "Audit log file should exist");

    let content = fs::read_to_string(&audit_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 3, "Should have 3 audit entries");

    assert!(lines[0].contains("agent_started"));
    assert!(lines[1].contains("hook_error"));
    assert!(lines[2].contains("agent_stopped"));
}

#[test]
fn test_audit_logger_buffer_auto_flush() {
    let temp_dir = TempDir::new().unwrap();
    let mut logger = AuditLogger::new(temp_dir.path())
        .unwrap()
        .with_max_buffer_size(2);

    logger.log(AuditEntry::new(
        AuditEventType::PermissionGranted,
        AuditSeverity::Info,
        serde_json::json!({}),
        "s1".to_string(),
    ));
    logger.log(AuditEntry::new(
        AuditEventType::PermissionDenied,
        AuditSeverity::Warning,
        serde_json::json!({}),
        "s1".to_string(),
    ));

    let audit_path = temp_dir.path().join(".synthia").join("audit.log");
    assert!(
        audit_path.exists(),
        "Auto-flush should have created the file"
    );
}

#[test]
fn test_audit_entry_serialization_roundtrip() {
    let entry = AuditEntry::new(
        AuditEventType::CredentialRedacted,
        AuditSeverity::Critical,
        serde_json::json!({"credential_type": "api_key", "masked": "****1234"}),
        "session-42".to_string(),
    );

    let json = serde_json::to_string(&entry).unwrap();
    let deserialized: AuditEntry = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.event_type, AuditEventType::CredentialRedacted);
    assert_eq!(deserialized.severity, AuditSeverity::Critical);
    assert_eq!(deserialized.session_id, "session-42");
}
