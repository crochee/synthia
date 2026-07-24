use crate::types::{SecurityEvent, SecurityEventType, SecuritySeverity};

/// Sandbox execution result
#[derive(Debug)]
pub struct SandboxCheckResult {
    /// Whether the operation is allowed
    pub allowed: bool,
    /// Reason if denied
    pub reason: Option<String>,
    /// Security severity level if denied
    pub severity: Option<SecuritySeverity>,
}

impl SandboxCheckResult {
    /// Creates an allowed result
    #[must_use]
    pub fn allowed() -> Self {
        Self {
            allowed: true,
            reason: None,
            severity: None,
        }
    }

    /// Creates a denied result with reason
    #[must_use]
    pub fn denied(reason: String, severity: SecuritySeverity) -> Self {
        Self {
            allowed: false,
            reason: Some(reason),
            severity: Some(severity),
        }
    }

    /// Converts to a security event if denied
    pub fn to_event(&self) -> Option<SecurityEvent> {
        if self.allowed {
            return None;
        }

        Some(SecurityEvent::new(
            SecurityEventType::SandboxViolation,
            self.reason.clone().unwrap_or_default(),
            None,
            self.severity.clone().unwrap_or(SecuritySeverity::High),
        ))
    }
}
