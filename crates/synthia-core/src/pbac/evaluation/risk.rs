//! Standard risk evaluator for PBAC access requests.

use super::super::context::{AccessRequest, RiskAssessment};

pub struct StandardRiskEvaluator;

impl StandardRiskEvaluator {
    pub fn evaluate(&self, request: &AccessRequest) -> RiskAssessment {
        let mut assessment = RiskAssessment::new(0.0);

        if request.action.is_execute() {
            assessment = assessment.with_factor(
                "bash_execution",
                0.4,
                0.8,
                "Bash/command execution detected",
            );
        }

        if request.resource.sensitivity_level.unwrap_or(0) > 7 {
            assessment = assessment.with_factor(
                "high_sensitivity",
                0.3,
                0.9,
                "High sensitivity resource",
            );
        }

        if !request.environment.is_business_hours() {
            assessment = assessment.with_factor(
                "off_hours",
                0.2,
                0.6,
                "Operation outside business hours",
            );
        }

        if request.subject.clearance_level.unwrap_or(0)
            < request.resource.sensitivity_level.unwrap_or(0)
        {
            assessment = assessment.with_factor(
                "insufficient_clearance",
                0.5,
                1.0,
                "Subject lacks required clearance",
            );
        }

        assessment.score = assessment.compute_weighted_score();
        assessment
    }
}
