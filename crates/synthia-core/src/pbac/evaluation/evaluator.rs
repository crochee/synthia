//! Policy evaluator - evaluates access requests against policies.

use std::sync::Arc;

use super::{
    super::{
        context::{AccessRequest, RiskRecommendation},
        policy::{Policy, PolicyResult},
    },
    audit::ConsoleAuditLogger,
    risk::StandardRiskEvaluator,
    types::{AuditInfo, EvaluationDecision, EvaluationResult, FailedCondition},
};

pub struct PolicyEvaluator {
    policies: Vec<Arc<dyn Policy>>,
    risk_evaluator: Option<Box<StandardRiskEvaluator>>,
    audit_logger: Option<Box<ConsoleAuditLogger>>,
}

impl PolicyEvaluator {
    pub fn new() -> Self {
        Self {
            policies: Vec::new(),
            risk_evaluator: None,
            audit_logger: None,
        }
    }

    pub fn with_policies<P: Policy + 'static>(
        mut self,
        policies: Vec<P>,
    ) -> Self {
        for p in policies {
            self.policies.push(Arc::new(p));
        }
        self
    }

    pub fn with_risk_evaluator(
        mut self,
        evaluator: StandardRiskEvaluator,
    ) -> Self {
        self.risk_evaluator = Some(Box::new(evaluator));
        self
    }

    pub fn with_audit_logger(mut self, logger: ConsoleAuditLogger) -> Self {
        self.audit_logger = Some(Box::new(logger));
        self
    }

    pub fn evaluate(&self, request: &AccessRequest) -> EvaluationResult {
        let start = std::time::Instant::now();

        let mut matched_policies = Vec::new();
        let mut failed_conditions = Vec::new();
        let mut any_matched = false;

        for policy in &self.policies {
            match policy.matches(request) {
                PolicyResult::Match => {
                    any_matched = true;
                    matched_policies.push(policy.name().to_string());

                    if let Some(conditions) = policy.conditions() {
                        for condition in conditions {
                            if condition.requires_confirmation {
                                failed_conditions.push(FailedCondition {
                                    policy: policy.name().to_string(),
                                    condition: condition.name.clone(),
                                });
                            }
                        }
                    }
                }
                PolicyResult::NoMatch => {}
                PolicyResult::Indeterminate(reason) => {
                    if let Some(ref logger) = self.audit_logger {
                        logger.log_indeterminate(policy.name(), &reason);
                    }
                    return EvaluationResult::deny(&reason).with_audit_info(
                        AuditInfo {
                            timestamp: chrono::Utc::now(),
                            request_id: request.subject.session_id.clone(),
                            evaluation_time_ms: start.elapsed().as_millis()
                                as u64,
                            risk_assessment: None,
                        },
                    );
                }
            }
        }

        let risk_assessment =
            self.risk_evaluator.as_ref().map(|e| e.evaluate(request));

        let decision = if !any_matched {
            EvaluationDecision::NotApplicable
        } else if let Some(ref assessment) = risk_assessment {
            match assessment.recommendation {
                RiskRecommendation::Deny => EvaluationDecision::Deny,
                _ => {
                    if failed_conditions.is_empty() {
                        EvaluationDecision::Permit
                    } else {
                        EvaluationDecision::Deny
                    }
                }
            }
        } else if failed_conditions.is_empty() {
            EvaluationDecision::Permit
        } else {
            EvaluationDecision::Deny
        };

        let result = EvaluationResult {
            decision,
            matched_policies,
            failed_conditions,
            audit_info: AuditInfo {
                timestamp: chrono::Utc::now(),
                request_id: request.subject.session_id.clone(),
                evaluation_time_ms: start.elapsed().as_millis() as u64,
                risk_assessment,
            },
        };

        if let Some(ref logger) = self.audit_logger {
            logger.log_evaluation(&result);
        }

        result
    }

    pub fn evaluate_sync(&self, request: &AccessRequest) -> EvaluationResult {
        self.evaluate(request)
    }
}

impl Default for PolicyEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

pub struct PolicyEvaluatorBuilder {
    policies: Vec<Arc<dyn Policy>>,
    risk_evaluator: Option<Box<StandardRiskEvaluator>>,
    audit_logger: Option<Box<ConsoleAuditLogger>>,
}

impl PolicyEvaluatorBuilder {
    pub fn new() -> Self {
        Self {
            policies: Vec::new(),
            risk_evaluator: None,
            audit_logger: None,
        }
    }

    pub fn policy<P: Policy + 'static>(mut self, policy: P) -> Self {
        self.policies.push(Arc::new(policy));
        self
    }

    pub fn risk_evaluator(mut self, evaluator: StandardRiskEvaluator) -> Self {
        self.risk_evaluator = Some(Box::new(evaluator));
        self
    }

    pub fn audit_logger(mut self, logger: ConsoleAuditLogger) -> Self {
        self.audit_logger = Some(Box::new(logger));
        self
    }

    pub fn build(self) -> PolicyEvaluator {
        let mut evaluator = PolicyEvaluator::new();
        evaluator.policies = self.policies;
        evaluator.risk_evaluator = self.risk_evaluator;
        evaluator.audit_logger = self.audit_logger;
        evaluator
    }
}

impl Default for PolicyEvaluatorBuilder {
    fn default() -> Self {
        Self::new()
    }
}
