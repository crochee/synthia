//! The [`AuditEntry`] struct — a single audit log record.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{event_type::AuditEventType, severity::AuditSeverity};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: String,
    pub event_type: AuditEventType,
    pub severity: AuditSeverity,
    pub detail: Value,
    pub session_id: String,
}

impl AuditEntry {
    pub fn new(
        event_type: AuditEventType,
        severity: AuditSeverity,
        detail: Value,
        session_id: String,
    ) -> Self {
        Self {
            timestamp: Utc::now().to_rfc3339(),
            event_type,
            severity,
            detail,
            session_id,
        }
    }
}
