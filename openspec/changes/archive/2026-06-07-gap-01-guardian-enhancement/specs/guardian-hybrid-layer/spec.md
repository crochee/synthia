## ADDED Requirements

### Requirement: Hybrid Guardian shall combine rule-based and LLM-based review

The Guardian system SHALL implement a hybrid architecture consisting of:
- SimpleGuardian for rule-based fast-path evaluation
- GuardianReviewer for LLM-based deep review on escalated requests

When a tool call is received, SimpleGuardian SHALL be consulted first for quick rule-based determination. If SimpleGuardian returns Allow with risk score < 50, the request SHALL proceed immediately. If risk score >= 50 or SimpleGuardian cannot determine, the request SHALL escalate to GuardianReviewer for LLM-based evaluation.

#### Scenario: Low-risk request passes fast-path
- **WHEN** SimpleGuardian evaluates a tool call with risk score < 50
- **THEN** Guardian SHALL return GuardianDecision::Allow immediately without LLM review

#### Scenario: High-risk request escalated to LLM review
- **WHEN** SimpleGuardian evaluates a tool call with risk score >= 50
- **THEN** Guardian SHALL escalate to GuardianReviewer for LLM-based evaluation

#### Scenario: GuardianReviewer fails and SimpleGuardian provides fallback
- **WHEN** GuardianReviewer times out or returns error
- **THEN** Guardian SHALL fall back to SimpleGuardian's rule-based decision with fail-closed default

---

### Requirement: Guardian shall implement fail-closed policy

When Guardian cannot determine safety (LLM unavailable, timeout, error), the Guardian SHALL deny the request by default. This ensures no dangerous operation proceeds due to Guardian unavailability.

#### Scenario: LLM service unavailable
- **WHEN** GuardianReviewer cannot reach LLM service
- **THEN** Guardian SHALL return GuardianDecision::Deny with reason "LLM service unavailable - fail closed"

#### Scenario: LLM review times out
- **WHEN** GuardianReviewer LLM call exceeds 30s timeout
- **THEN** Guardian SHALL fall back to SimpleGuardian; if SimpleGuardian also cannot determine, return GuardianDecision::Deny

---

### Requirement: Guardian shall expose check method via trait

The Guardian SHALL implement a `Guardian` trait with an async `check` method that accepts an ApprovalRequest and returns a GuardianDecision.

```rust
pub trait Guardian: Send + Sync {
    async fn check(&self, request: &ApprovalRequest) -> GuardianDecision;
}
```

#### Scenario: SimpleGuardian check returns decision
- **WHEN** SimpleGuardian.check() is called with ApprovalRequest
- **THEN** Guardian SHALL return GuardianDecision::Allow, GuardianDecision::Deny, or GuardianDecision::NeedUserConfirm

#### Scenario: GuardianReviewer check returns decision
- **WHEN** GuardianReviewer.check() is called with ApprovalRequest
- **THEN** Guardian SHALL return GuardianDecision::Allow or GuardianDecision::Deny (NeedUserConfirm not supported in v1)