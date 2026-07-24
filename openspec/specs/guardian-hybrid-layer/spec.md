# guardian-hybrid-layer Specification

## Purpose
TBD - created by archiving change gap-01-guardian-enhancement. Update Purpose after archive.
## Requirements
### Requirement: Hybrid Guardian shall combine rule-based and LLM-based review

The Guardian system SHALL implement a hybrid architecture consisting of:
- SimpleGuardian for rule-based fast-path evaluation
- GuardianSubagentReviewer for LLM-based deep review on escalated requests, running as an isolated subagent via `SubagentSessionFactory`

When a tool call is received, SimpleGuardian SHALL be consulted first for quick rule-based determination. The escalation tiers SHALL be:
- Risk score < 50: SimpleGuardian SHALL return `GuardianDecision::Allow` immediately (fast-path, no subagent spawn)
- Risk score >= 80: SimpleGuardian SHALL return `GuardianDecision::Deny` immediately (fast-path, no subagent spawn)
- Risk score in [50, 80): the request SHALL escalate to `GuardianSubagentReviewer` for LLM-based evaluation in an isolated subagent session

When `GuardianConfig::subagent_enabled` is `false` (default), the [50, 80) tier SHALL fall back to `SimpleGuardian::NeedUserConfirm` without spawning a subagent (legacy behavior).

#### Scenario: Low-risk request passes fast-path
- **WHEN** SimpleGuardian evaluates a tool call with risk score < 50
- **THEN** Guardian SHALL return `GuardianDecision::Allow` immediately without LLM review or subagent spawn

#### Scenario: High-risk request denied on fast-path
- **WHEN** SimpleGuardian evaluates a tool call with risk score >= 80
- **THEN** Guardian SHALL return `GuardianDecision::Deny` immediately without LLM review or subagent spawn

#### Scenario: Medium-risk request escalated to Guardian subagent
- **WHEN** SimpleGuardian evaluates a tool call with risk score in [50, 80) and `subagent_enabled` is `true`
- **THEN** Guardian SHALL escalate to `GuardianSubagentReviewer` which spawns an isolated subagent for LLM-based evaluation

#### Scenario: Medium-risk request falls back when subagent disabled
- **WHEN** SimpleGuardian evaluates a tool call with risk score in [50, 80) and `subagent_enabled` is `false`
- **THEN** Guardian SHALL return `GuardianDecision::NeedUserConfirm` without spawning a subagent (legacy behavior)

#### Scenario: Guardian subagent fails and SimpleGuardian provides fallback
- **WHEN** GuardianSubagentReviewer times out, returns error, or is cancelled
- **THEN** Guardian SHALL fall back to `SimpleGuardian::NeedUserConfirm` (fail-closed for medium-risk; user confirmation required)

---

### Requirement: Guardian shall implement fail-closed policy

When Guardian cannot determine safety (subagent unavailable, timeout, error, cancellation), the Guardian SHALL fall back to `SimpleGuardian`'s rule-based decision. For medium-risk requests (risk in [50, 80)) where the subagent fails, the fallback SHALL be `GuardianDecision::NeedUserConfirm` (user confirmation required, not outright deny, because the rule engine already classified the request as non-high-risk). For high-risk requests (risk >= 80), `SimpleGuardian::Deny` is the fast-path and no subagent is involved. The subagent review timeout SHALL be configurable via `GuardianConfig::timeout` (default 90s).

#### Scenario: Subagent service unavailable
- **WHEN** `GuardianSubagentReviewer` cannot spawn a subagent (e.g., `SubagentSessionFactory` returns error)
- **THEN** Guardian SHALL fall back to `SimpleGuardian::NeedUserConfirm` for medium-risk requests

#### Scenario: Subagent review times out
- **WHEN** Guardian subagent review exceeds `GuardianConfig::timeout` (default 90s)
- **THEN** Guardian SHALL fall back to `SimpleGuardian::NeedUserConfirm` for medium-risk requests

#### Scenario: Subagent cancelled by parent session abort
- **WHEN** the parent session's `CancellationToken` is triggered during Guardian subagent review
- **THEN** Guardian SHALL fall back to `SimpleGuardian::NeedUserConfirm` for medium-risk requests

#### Scenario: Subagent output parsing fails
- **WHEN** the Guardian subagent's `Finish` output cannot be parsed as a JSON assessment
- **THEN** Guardian SHALL fall back to `SimpleGuardian::NeedUserConfirm` for medium-risk requests

---

### Requirement: Guardian shall expose check method via trait

The Guardian SHALL implement a `Guardian` trait with an async `check` method that accepts an `ApprovalRequest`, a conversation transcript slice, a `CancellationToken`, and an optional `SubagentSessionFactory` for subagent-based review. The method SHALL return a `GuardianDecision`.

```rust
pub trait Guardian: Send + Sync {
    async fn check(
        &self,
        request: &ApprovalRequest,
        conversation: &[Message],
        cancel_token: CancellationToken,
        subagent_factory: Option<&dyn SubagentSessionFactory>,
    ) -> GuardianDecision;
}
```

When `subagent_factory` is `None` or `GuardianConfig::subagent_enabled` is `false`, the check SHALL use the legacy path (SimpleGuardian fast-path + `NeedUserConfirm` for medium-risk). When `subagent_factory` is `Some` and `subagent_enabled` is `true`, medium-risk requests SHALL escalate to the Guardian subagent.

#### Scenario: SimpleGuardian check returns decision for low-risk
- **WHEN** `Guardian::check` is called with a low-risk `ApprovalRequest` (risk < 50)
- **THEN** Guardian SHALL return `GuardianDecision::Allow` immediately without invoking the subagent factory

#### Scenario: GuardianSubagentReviewer check returns decision for medium-risk
- **WHEN** `Guardian::check` is called with a medium-risk `ApprovalRequest` (risk in [50, 80)) and `subagent_enabled` is `true` and `subagent_factory` is `Some`
- **THEN** Guardian SHALL spawn a Guardian subagent via `SubagentSessionFactory::run_child`
- **AND** SHALL return `GuardianDecision::Allow`, `GuardianDecision::Deny`, or `GuardianDecision::NeedUserConfirm` based on the subagent's assessment

#### Scenario: Legacy check returns NeedUserConfirm for medium-risk without subagent
- **WHEN** `Guardian::check` is called with a medium-risk `ApprovalRequest` (risk in [50, 80)) and `subagent_enabled` is `false` or `subagent_factory` is `None`
- **THEN** Guardian SHALL return `GuardianDecision::NeedUserConfirm` without spawning a subagent
