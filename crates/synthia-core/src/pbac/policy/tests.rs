//! Unit tests for the `policy` module family.
//!
//! Coverage map (5 tests):
//!
//! - `RoleCheck` basic: 1 test ([`test_role_condition`]).
//! - `RiskThreshold` allow/deny: 1 test
//!   ([`test_risk_threshold_condition`]).
//! - `ConditionResult` accessors: 1 test
//!   ([`test_condition_result_is_allowed`]).
//! - `resolve_attribute`: 1 test ([`test_resolve_attribute`]).
//! - `RoleCheck` with `require_all=true`: 1 test
//!   ([`test_role_check_require_all`]).

use super::{
    super::context::{AccessRequest, EnvironmentAttributes, SubjectAttributes},
    resolve::resolve_attribute,
    *,
};

#[test]
fn test_role_condition() {
    let request = AccessRequest::new("user1", "bash", "execute")
        .with_subject(SubjectAttributes::new("user1").role("admin"));

    let condition = ConditionDefinition::RoleCheck {
        required_role: "admin".to_string(),
        require_all: Some(false),
    };

    assert!(condition.evaluate(&request).is_allowed());
}

#[test]
fn test_risk_threshold_condition() {
    let request = AccessRequest::new("user1", "bash", "execute")
        .with_environment(EnvironmentAttributes::new().risk_score(0.5));

    let condition = ConditionDefinition::RiskThreshold {
        max_risk_score: 0.7,
    };
    assert!(condition.evaluate(&request).is_allowed());

    let high_risk_condition = ConditionDefinition::RiskThreshold {
        max_risk_score: 0.3,
    };
    let high_risk_request = AccessRequest::new("user1", "bash", "execute")
        .with_environment(EnvironmentAttributes::new().risk_score(0.8));

    assert!(high_risk_condition.evaluate(&high_risk_request).is_denied());
}

#[test]
fn test_condition_result_is_allowed() {
    assert!(ConditionResult::Allowed.is_allowed());
    assert!(!ConditionResult::Denied("test".to_string()).is_allowed());
    assert!(!ConditionResult::Indeterminate("test".to_string()).is_allowed());
}

#[test]
fn test_resolve_attribute() {
    let request = AccessRequest::new("user1", "bash", "execute")
        .with_environment(EnvironmentAttributes::new().risk_score(0.3));

    assert_eq!(
        resolve_attribute("subject.id", &request),
        Some(serde_json::json!("user1"))
    );
    assert_eq!(
        resolve_attribute("environment.risk_score", &request),
        Some(serde_json::json!(0.3))
    );
}

#[test]
fn test_role_check_require_all() {
    let request_with_role = AccessRequest::new("user1", "bash", "execute")
        .with_subject(SubjectAttributes::new("user1").role("admin"));

    let request_without_role = AccessRequest::new("user2", "bash", "execute");

    let condition = ConditionDefinition::RoleCheck {
        required_role: "admin".to_string(),
        require_all: Some(true),
    };

    assert!(condition.evaluate(&request_with_role).is_allowed());
    assert!(condition.evaluate(&request_without_role).is_denied());
}
