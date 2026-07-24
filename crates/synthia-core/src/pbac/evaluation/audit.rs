//! Console audit logger for PBAC evaluation results.

use super::types::EvaluationResult;

pub struct ConsoleAuditLogger;

impl ConsoleAuditLogger {
    pub fn log_evaluation(&self, result: &EvaluationResult) {
        tracing::info!(
            decision = ?result.decision,
            matched_policies = ?result.matched_policies,
            evaluation_time_ms = result.audit_info.evaluation_time_ms,
            "PBAC evaluation completed"
        );
    }

    pub fn log_indeterminate(&self, policy: &str, reason: &str) {
        tracing::warn!(
            policy = policy,
            reason = reason,
            "PBAC evaluation indeterminate"
        );
    }
}
