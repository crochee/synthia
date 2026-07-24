#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::pbac::{policy::Condition, *};

    struct TestPolicy {
        name: String,
        conditions: Vec<Condition>,
    }

    impl Policy for TestPolicy {
        fn name(&self) -> &str {
            &self.name
        }

        fn matches(&self, _request: &AccessRequest) -> PolicyResult {
            PolicyResult::Match
        }

        fn conditions(&self) -> Option<Vec<Condition>> {
            Some(self.conditions.clone())
        }

        fn priority(&self) -> i32 {
            0
        }
    }

    #[test]
    fn test_evaluation_result_permit() {
        let result = EvaluationResult::permit(vec!["policy1", "policy2"]);
        assert!(result.is_permitted());
        assert!(!result.is_denied());
        assert_eq!(result.matched_policies.len(), 2);
    }

    #[test]
    fn test_evaluation_result_deny() {
        let result = EvaluationResult::deny("Missing role");
        assert!(!result.is_permitted());
        assert!(result.is_denied());
    }

    #[test]
    fn test_policy_evaluator_no_policies() {
        let evaluator = PolicyEvaluator::new();
        let request = AccessRequest::new("user1", "bash", "execute");
        let result = evaluator.evaluate(&request);
        assert_eq!(result.decision, EvaluationDecision::NotApplicable);
    }

    #[test]
    fn test_policy_evaluator_with_policy() {
        let evaluator = PolicyEvaluatorBuilder::new()
            .policy(TestPolicy {
                name: "test_policy".to_string(),
                conditions: Vec::new(),
            })
            .build();

        let request = AccessRequest::new("user1", "bash", "execute");
        let result = evaluator.evaluate(&request);
        assert!(result.is_permitted());
    }

    #[test]
    fn test_standard_risk_evaluator_bash() {
        let evaluator = StandardRiskEvaluator;
        let request = AccessRequest::new("user1", "bash", "execute");
        let assessment = evaluator.evaluate(&request);

        assert!(assessment.score > 0.0);
        assert!(!assessment.factors.is_empty());
    }

    #[test]
    fn test_failed_conditions_returns_deny() {
        let evaluator = PolicyEvaluatorBuilder::new()
            .policy(TestPolicy {
                name: "confirm_policy".to_string(),
                conditions: vec![
                    Condition::new("confirm_action", "Requires confirmation")
                        .with_confirmation("This action requires confirmation"),
                ],
            })
            .build();

        let request = AccessRequest::new("user1", "bash", "execute");
        let result = evaluator.evaluate(&request);
        assert_eq!(result.decision, EvaluationDecision::Deny);
        assert!(!result.failed_conditions.is_empty());
    }
}
