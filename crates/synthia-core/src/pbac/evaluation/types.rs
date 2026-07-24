//! PBAC evaluation types - result, decision, conditions, audit info.

use serde::{Deserialize, Serialize};

use super::super::context::RiskAssessment;

#[derive(Debug, Clone)]
pub struct EvaluationResult {
    pub decision: EvaluationDecision,
    pub matched_policies: Vec<String>,
    pub failed_conditions: Vec<FailedCondition>,
    pub audit_info: AuditInfo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EvaluationDecision {
    Permit,
    Deny,
    NotApplicable,
    Indeterminate,
}

impl EvaluationResult {
    pub fn permit(policies: Vec<&str>) -> Self {
        Self {
            decision: EvaluationDecision::Permit,
            matched_policies: policies.into_iter().map(String::from).collect(),
            failed_conditions: Vec::new(),
            audit_info: AuditInfo::default(),
        }
    }

    pub fn deny(reason: &str) -> Self {
        Self {
            decision: EvaluationDecision::Deny,
            matched_policies: Vec::new(),
            failed_conditions: vec![FailedCondition {
                policy: String::new(),
                condition: reason.to_string(),
            }],
            audit_info: AuditInfo::default(),
        }
    }

    pub fn with_audit_info(mut self, info: AuditInfo) -> Self {
        self.audit_info = info;
        self
    }

    pub fn is_permitted(&self) -> bool {
        matches!(self.decision, EvaluationDecision::Permit)
    }

    pub fn is_denied(&self) -> bool {
        matches!(self.decision, EvaluationDecision::Deny)
    }
}

#[derive(Debug, Clone)]
pub struct FailedCondition {
    pub policy: String,
    pub condition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditInfo {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub request_id: Option<String>,
    pub evaluation_time_ms: u64,
    pub risk_assessment: Option<RiskAssessment>,
}

impl Default for AuditInfo {
    fn default() -> Self {
        Self {
            timestamp: chrono::Utc::now(),
            request_id: None,
            evaluation_time_ms: 0,
            risk_assessment: None,
        }
    }
}
