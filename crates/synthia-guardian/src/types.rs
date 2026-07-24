//! Guardian types for safety guardrails

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Permission level for tool execution
///
/// Follows the fail-closed principle: if a permission check times out,
/// the operation is denied by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionLevel {
    /// Auto-approved without user confirmation (read-only operations)
    AutoApprove,
    /// Requires quick confirmation (press Enter or type 'y')
    RequireConfirm,
    /// Requires explicit approval (type "yes" fully)
    RequireExplicit,
    /// Blocked - tool execution is prevented
    Block,
}

impl PermissionLevel {
    /// Returns the numeric priority for comparison (higher = more restrictive)
    pub fn priority(&self) -> u8 {
        match self {
            Self::AutoApprove => 0,
            Self::RequireConfirm => 1,
            Self::RequireExplicit => 2,
            Self::Block => 3,
        }
    }

    /// Returns true if this level requires user interaction
    pub fn requires_user_action(&self) -> bool {
        matches!(
            self,
            Self::RequireConfirm | Self::RequireExplicit | Self::Block
        )
    }
}

/// Guardian state snapshot for monitoring and debugging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianState {
    /// Loop detection counters by detector type
    pub loop_detection_counts: HashMap<String, usize>,
    /// Consecutive no-progress count
    pub no_progress_count: usize,
    /// Consecutive error count
    pub consecutive_errors: usize,
    /// Whether the circuit breaker is open
    pub circuit_breaker_open: bool,
    /// Total security events recorded
    pub total_security_events: usize,
}

impl GuardianState {
    /// Creates a new guardian state with default values
    #[must_use]
    pub fn new() -> Self {
        Self {
            loop_detection_counts: HashMap::new(),
            no_progress_count: 0,
            consecutive_errors: 0,
            circuit_breaker_open: false,
            total_security_events: 0,
        }
    }
}

impl Default for GuardianState {
    fn default() -> Self {
        Self::new()
    }
}

/// Loop detection result from any of the four detector layers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopDetectionResult {
    /// Whether a loop was detected
    pub detected: bool,
    /// Detector type identifier (GenericRepeat, PollNoProgress, PingPong, GlobalCircuit)
    pub detector_type: String,
    /// Hash of the repeated call pattern (empty for some detectors)
    pub hash: String,
    /// Repeat count
    pub count: usize,
    /// Severity of the detection (warning vs block threshold)
    pub severity: SecuritySeverity,
}

impl LoopDetectionResult {
    /// Creates a new detection result indicating no loop detected
    #[must_use]
    pub fn not_detected(detector_type: &str, count: usize) -> Self {
        Self {
            detected: false,
            detector_type: detector_type.to_string(),
            hash: String::new(),
            count,
            severity: SecuritySeverity::Low,
        }
    }

    /// Creates a new detection result indicating a loop was detected
    #[must_use]
    pub fn detected(
        detector_type: &str,
        hash: String,
        count: usize,
        severity: SecuritySeverity,
    ) -> Self {
        Self {
            detected: true,
            detector_type: detector_type.to_string(),
            hash,
            count,
            severity,
        }
    }

    /// Returns true if this detection should block execution
    pub fn should_block(&self) -> bool {
        self.detected
            && matches!(
                self.severity,
                SecuritySeverity::High | SecuritySeverity::Critical
            )
    }
}

/// Types of security events that can be recorded
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityEventType {
    /// Loop pattern detected by any of the four detectors
    LoopDetected,
    /// Circuit breaker triggered due to consecutive failures
    CircuitBreakerTriggered,
    /// Prompt injection attempt detected
    InjectionDetected,
    /// Permission denied for tool execution
    PermissionDenied,
    /// Sandbox constraint violation
    SandboxViolation,
    /// Credential leak detected in output
    CredentialLeak,
}

/// Security event record for auditing and monitoring
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecurityEvent {
    /// Type of the security event
    pub event_type: SecurityEventType,
    /// Timestamp when the event occurred
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Human-readable description of the event
    pub description: String,
    /// Name of the tool involved (if applicable)
    pub tool_name: Option<String>,
    /// Severity level of the event
    pub severity: SecuritySeverity,
}

impl SecurityEvent {
    /// Creates a new security event with the current timestamp
    #[must_use]
    pub fn new(
        event_type: SecurityEventType,
        description: String,
        tool_name: Option<String>,
        severity: SecuritySeverity,
    ) -> Self {
        Self {
            event_type,
            timestamp: chrono::Utc::now(),
            description,
            tool_name,
            severity,
        }
    }
}

/// Severity levels for security events and detections
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecuritySeverity {
    /// Informational or low-impact detection
    Low,
    /// Moderate concern that should be monitored
    Medium,
    /// High-risk detection that may require blocking
    High,
    /// Critical security violation requiring immediate action
    Critical,
}

impl SecuritySeverity {
    /// Returns the numeric priority for comparison
    pub fn priority(&self) -> u8 {
        match self {
            Self::Low => 0,
            Self::Medium => 1,
            Self::High => 2,
            Self::Critical => 3,
        }
    }
}

/// Loop detection status returned by individual detectors.
///
/// A three-state enum that complements `LoopAction`. Each detector
/// returns one of these values; the set combines them into a single
/// `(LoopStatus, Option<LoopAction>)` pair via `LoopDetectorSet::check`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LoopStatus {
    /// No loop pattern observed.
    Ok,
    /// Loop pattern is forming (e.g. count is one below the block threshold).
    /// The caller MAY log a warning but SHOULD continue execution.
    Warning,
    /// Loop pattern is confirmed. The caller SHOULD NOT execute the tool
    /// without first consulting the accompanying `LoopAction`.
    Detected,
}

/// Recommended caller action for a loop detection result.
///
/// Returned alongside `LoopStatus` to disambiguate how the caller
/// should respond. Variants are unit (no payload) so they can be
/// cheaply cloned/copied in hot paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LoopAction {
    /// No loop detected; proceed normally.
    Continue,
    /// GenericRepeat near threshold: log a warning and proceed.
    Warn,
    /// Standard block (GenericRepeat, PingPong, PollNoProgress):
    /// skip execution for this call.
    Block,
    /// DoomLoop: invoke `synthia_permission::Permission::ask` before
    /// deciding whether to execute. Mirrors opencode's `doom_loop`
    /// permission category.
    RequirePermission,
    /// GlobalCircuit: terminate the entire agent loop (Critical severity).
    HardBlock,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_level_priority() {
        assert!(
            PermissionLevel::Block.priority()
                > PermissionLevel::AutoApprove.priority()
        );
        assert!(
            PermissionLevel::RequireExplicit.priority()
                > PermissionLevel::RequireConfirm.priority()
        );
    }

    #[test]
    fn test_permission_level_requires_user_action() {
        assert!(!PermissionLevel::AutoApprove.requires_user_action());
        assert!(PermissionLevel::RequireConfirm.requires_user_action());
        assert!(PermissionLevel::RequireExplicit.requires_user_action());
        assert!(PermissionLevel::Block.requires_user_action());
    }

    #[test]
    fn test_loop_detection_result_not_detected() {
        let result = LoopDetectionResult::not_detected("GenericRepeat", 0);
        assert!(!result.detected);
        assert_eq!(result.detector_type, "GenericRepeat");
        assert!(!result.should_block());
    }

    #[test]
    fn test_loop_detection_result_detected() {
        let result = LoopDetectionResult::detected(
            "GenericRepeat",
            "abc123".to_string(),
            5,
            SecuritySeverity::High,
        );
        assert!(result.detected);
        assert!(result.should_block());
    }

    #[test]
    fn test_guardian_state_default() {
        let state = GuardianState::default();
        assert!(!state.circuit_breaker_open);
        assert_eq!(state.consecutive_errors, 0);
        assert_eq!(state.total_security_events, 0);
    }

    #[test]
    fn test_security_event_creation() {
        let event = SecurityEvent::new(
            SecurityEventType::LoopDetected,
            "Test detection".to_string(),
            Some("bash".to_string()),
            SecuritySeverity::High,
        );
        assert_eq!(event.tool_name, Some("bash".to_string()));
        assert!(matches!(event.event_type, SecurityEventType::LoopDetected));
    }

    #[test]
    fn test_loop_action_serde_roundtrip() {
        let original = LoopAction::RequirePermission;
        let json = serde_json::to_string(&original).expect("serialize");
        assert_eq!(json, "\"RequirePermission\"");
        let decoded: LoopAction =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_loop_action_distinct_variants() {
        // Ensure RequirePermission and HardBlock are distinct
        // (semantically different caller responses).
        assert_ne!(
            (LoopStatus::Detected, Some(LoopAction::RequirePermission)),
            (LoopStatus::Detected, Some(LoopAction::HardBlock))
        );
        // And both are distinct from Block.
        assert_ne!(
            (LoopStatus::Detected, Some(LoopAction::RequirePermission)),
            (LoopStatus::Detected, Some(LoopAction::Block))
        );
    }
}
