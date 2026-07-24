//! The [`AuditLogger`] struct — the buffered logger that drains
//! to disk at `max_buffer_size` (default 100) or on explicit
//! `flush()`. Contains the 7 typed helper methods
//! ([`AuditLogger::log_permission_granted`],
//!  [`AuditLogger::log_permission_denied`],
//!  [`AuditLogger::log_input_blocked`],
//!  [`AuditLogger::log_output_blocked`],
//!  [`AuditLogger::log_credential_redacted`],
//!  [`AuditLogger::log_loop_detected`],
//!  [`AuditLogger::log_circuit_breaker`]).

use std::{
    fs::{OpenOptions, create_dir_all},
    io::Write,
    path::{Path, PathBuf},
};

use synthia_core::Error;

use super::{
    entry::AuditEntry,
    event_type::AuditEventType,
    severity::AuditSeverity,
};

const DEFAULT_MAX_BUFFER_SIZE: usize = 100;

pub struct AuditLogger {
    buffer: Vec<AuditEntry>,
    max_buffer_size: usize,
    log_path: PathBuf,
}

impl AuditLogger {
    /// Creates a new AuditLogger that writes to
    /// `.synthia/audit.log` in the workspace root. Creates the
    /// `.synthia` directory if it does not exist.
    pub fn new(workspace_root: &Path) -> Result<Self, Error> {
        let synthia_dir = workspace_root.join(".synthia");
        create_dir_all(&synthia_dir)?;

        let log_path = synthia_dir.join("audit.log");

        Ok(Self {
            buffer: Vec::with_capacity(DEFAULT_MAX_BUFFER_SIZE),
            max_buffer_size: DEFAULT_MAX_BUFFER_SIZE,
            log_path,
        })
    }

    /// Creates a new AuditLogger with a custom log path (for testing).
    #[cfg(test)]
    pub fn with_path(log_path: PathBuf) -> Self {
        Self {
            buffer: Vec::with_capacity(DEFAULT_MAX_BUFFER_SIZE),
            max_buffer_size: DEFAULT_MAX_BUFFER_SIZE,
            log_path,
        }
    }

    /// Sets the maximum buffer size before automatic flush.
    pub fn with_max_buffer_size(mut self, size: usize) -> Self {
        self.max_buffer_size = size;
        self
    }

    /// Logs an audit entry. If the buffer reaches max capacity, flushes first.
    pub fn log(&mut self, entry: AuditEntry) {
        self.buffer.push(entry);
        if self.buffer.len() >= self.max_buffer_size {
            let _ = self.flush();
        }
    }

    /// Flushes all buffered entries to the audit log file. Called on agent stop.
    pub fn flush(&mut self) -> Result<(), Error> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;

        for entry in self.buffer.drain(..) {
            let json = serde_json::to_string(&entry)?;
            writeln!(file, "{}", json)?;
        }

        Ok(())
    }

    // --- Helper methods (Task 8.4) ---

    pub fn log_permission_granted(
        &mut self,
        session_id: &str,
        tool_name: &str,
        reason: &str,
    ) {
        let detail = serde_json::json!({
            "tool_name": tool_name,
            "reason": reason
        });
        self.log(AuditEntry::new(
            AuditEventType::PermissionGranted,
            AuditSeverity::Info,
            detail,
            session_id.to_string(),
        ));
    }

    pub fn log_permission_denied(
        &mut self,
        session_id: &str,
        tool_name: &str,
        reason: &str,
    ) {
        let detail = serde_json::json!({
            "tool_name": tool_name,
            "reason": reason
        });
        self.log(AuditEntry::new(
            AuditEventType::PermissionDenied,
            AuditSeverity::Warning,
            detail,
            session_id.to_string(),
        ));
    }

    pub fn log_input_blocked(
        &mut self,
        session_id: &str,
        pattern_matched: &str,
    ) {
        let detail = serde_json::json!({
            "pattern_matched": pattern_matched
        });
        self.log(AuditEntry::new(
            AuditEventType::InputBlocked,
            AuditSeverity::Warning,
            detail,
            session_id.to_string(),
        ));
    }

    pub fn log_output_blocked(
        &mut self,
        session_id: &str,
        pattern_matched: &str,
    ) {
        let detail = serde_json::json!({
            "pattern_matched": pattern_matched
        });
        self.log(AuditEntry::new(
            AuditEventType::OutputBlocked,
            AuditSeverity::Warning,
            detail,
            session_id.to_string(),
        ));
    }

    pub fn log_credential_redacted(
        &mut self,
        session_id: &str,
        credential_type: &str,
    ) {
        let detail = serde_json::json!({
            "credential_type": credential_type
        });
        self.log(AuditEntry::new(
            AuditEventType::CredentialRedacted,
            AuditSeverity::Info,
            detail,
            session_id.to_string(),
        ));
    }

    pub fn log_loop_detected(
        &mut self,
        session_id: &str,
        loop_type: &str,
        action_taken: &str,
    ) {
        let detail = serde_json::json!({
            "loop_type": loop_type,
            "action_taken": action_taken
        });
        self.log(AuditEntry::new(
            AuditEventType::LoopDetected,
            AuditSeverity::Warning,
            detail,
            session_id.to_string(),
        ));
    }

    pub fn log_circuit_breaker(
        &mut self,
        session_id: &str,
        service: &str,
        state: &str,
    ) {
        let detail = serde_json::json!({
            "service": service,
            "state": state
        });
        self.log(AuditEntry::new(
            AuditEventType::CircuitBreakerTriggered,
            AuditSeverity::Error,
            detail,
            session_id.to_string(),
        ));
    }
}
