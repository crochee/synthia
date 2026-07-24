//! The [`PolicySet`] container + its `evaluate` method
//! (4 combining algorithms) + the [`CombiningAlgorithm`] enum.

use std::sync::Arc;

use super::{
    super::context::AccessRequest,
    condition::Condition,
    policy_trait::{Policy, PolicyResult},
};

#[derive(Clone)]
pub struct PolicySet {
    policies: Vec<Arc<dyn Policy>>,
    combining_algorithm: CombiningAlgorithm,
}

impl PolicySet {
    pub fn new(combining_algorithm: CombiningAlgorithm) -> Self {
        Self {
            policies: Vec::new(),
            combining_algorithm,
        }
    }

    pub fn add_policy<P: Policy + 'static>(mut self, policy: P) -> Self {
        self.policies.push(Arc::new(policy));
        self
    }

    pub fn evaluate(&self, request: &AccessRequest) -> PolicyResult {
        let mut results: Vec<(PolicyResult, Option<Vec<Condition>>)> =
            Vec::new();

        for policy in &self.policies {
            let result = policy.matches(request);
            let conditions = policy.conditions();
            results.push((result, conditions));
        }

        match self.combining_algorithm {
            CombiningAlgorithm::DenyOverrides => {
                for (result, _) in &results {
                    if matches!(result, PolicyResult::Indeterminate(_)) {
                        return result.clone();
                    }
                }
                for (result, _) in &results {
                    if matches!(result, PolicyResult::NoMatch) {
                        continue;
                    }
                    if matches!(result, PolicyResult::Match) {
                        return PolicyResult::Match;
                    }
                }
                PolicyResult::NoMatch
            }
            CombiningAlgorithm::PermitOverrides => {
                for (result, _) in &results {
                    if matches!(result, PolicyResult::Indeterminate(_)) {
                        return result.clone();
                    }
                }
                for (result, _) in &results {
                    if matches!(result, PolicyResult::Match) {
                        return PolicyResult::Match;
                    }
                }
                PolicyResult::NoMatch
            }
            CombiningAlgorithm::FirstApplicable => {
                if let Some((result, _)) = results.first() {
                    return result.clone();
                }
                PolicyResult::NoMatch
            }
            CombiningAlgorithm::AllApplicable => {
                if results
                    .iter()
                    .all(|(r, _)| matches!(r, PolicyResult::Match))
                {
                    PolicyResult::Match
                } else {
                    PolicyResult::NoMatch
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CombiningAlgorithm {
    #[default]
    DenyOverrides,
    PermitOverrides,
    FirstApplicable,
    AllApplicable,
}
