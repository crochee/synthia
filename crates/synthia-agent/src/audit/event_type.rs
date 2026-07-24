//! The [`AuditEventType`] enum — 10 variants covering all
//! security-relevant events recorded by the audit pipeline.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    PermissionGranted,
    PermissionDenied,
    InputBlocked,
    OutputBlocked,
    CredentialRedacted,
    LoopDetected,
    CircuitBreakerTriggered,
    AgentStarted,
    AgentStopped,
    HookError,
}

impl fmt::Display for AuditEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuditEventType::PermissionGranted => {
                write!(f, "permission_granted")
            }
            AuditEventType::PermissionDenied => write!(f, "permission_denied"),
            AuditEventType::InputBlocked => write!(f, "input_blocked"),
            AuditEventType::OutputBlocked => write!(f, "output_blocked"),
            AuditEventType::CredentialRedacted => {
                write!(f, "credential_redacted")
            }
            AuditEventType::LoopDetected => write!(f, "loop_detected"),
            AuditEventType::CircuitBreakerTriggered => {
                write!(f, "circuit_breaker_triggered")
            }
            AuditEventType::AgentStarted => write!(f, "agent_started"),
            AuditEventType::AgentStopped => write!(f, "agent_stopped"),
            AuditEventType::HookError => write!(f, "hook_error"),
        }
    }
}
