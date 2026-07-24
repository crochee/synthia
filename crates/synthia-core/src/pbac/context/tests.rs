use super::*;

#[test]
fn test_subject_has_role() {
    let subject = SubjectAttributes::new("user1").role("admin").role("user");
    assert!(subject.has_role("admin"));
    assert!(subject.has_role("user"));
    assert!(!subject.has_role("superuser"));
}

#[test]
fn test_subject_wildcard_role() {
    let subject = SubjectAttributes::new("user1").role("*");
    assert!(subject.has_role("anything"));
}

#[test]
fn test_action_is_write() {
    let action = ActionAttributes::new("write_file");
    assert!(action.is_write());

    let action2 = ActionAttributes::new("edit_file").action_type("write");
    assert!(action2.is_write());
}

#[test]
fn test_risk_assessment() {
    let assessment = RiskAssessment::new(0.5)
        .with_factor("network", 0.3, 0.8, "External network")
        .with_factor("time", 0.2, 0.2, "Business hours")
        .with_factor("resource", 0.5, 0.4, "Standard resource");

    let weighted = assessment.compute_weighted_score();
    assert!((weighted - 0.48).abs() < 0.01);
}

#[test]
fn test_access_request_builder() {
    let request = AccessRequest::new("user1", "bash", "execute")
        .with_subject(
            SubjectAttributes::new("user1")
                .role("admin")
                .clearance_level(5),
        )
        .with_resource(
            ResourceAttributes::new("critical_file").sensitivity_level(10),
        )
        .with_environment(EnvironmentAttributes::new().risk_score(0.2))
        .with_context("source", "web_ui");

    assert_eq!(request.subject.id, "user1");
    assert!(request.subject.has_role("admin"));
    assert!(request.resource.sensitivity_level.unwrap() >= 5);
}
