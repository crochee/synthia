## ADDED Requirements

### Requirement: Guardian shall route confirmation based on action type

The Guardian SHALL support differentiated user confirmation strategies based on the type of action being reviewed:

- Shell/Exec operations: Blocking confirmation (agent pauses until user responds)
- Network operations: Non-blocking confirmation (agent continues with degraded capability)
- Credential operations: Interrupt confirmation (session pauses, checkpoint saved)

#### Scenario: Shell operation requires blocking confirmation
- **WHEN** Guardian receives ApprovalRequest for shell command (exec, bash, sudo)
- **THEN** Guardian SHALL return GuardianDecision::NeedUserConfirm with timeout=5min and blocking flag

#### Scenario: Network operation allows non-blocking confirmation
- **WHEN** Guardian receives ApprovalRequest for network access (curl, fetch, API call)
- **THEN** Guardian SHALL return GuardianDecision::NeedUserConfirm with non-blocking flag, agent continues

#### Scenario: Credential operation triggers interrupt
- **WHEN** Guardian receives ApprovalRequest for credential access (env vars, secrets, keys)
- **THEN** Guardian SHALL return GuardianDecision::NeedUserConfirm with interrupt flag, session state saved to checkpoint

---

### Requirement: User confirmation shall timeout after configurable duration

When GuardianDecision::NeedUserConfirm is returned, the confirmation SHALL timeout after a configurable duration (default: 5 minutes). On timeout, the request SHALL be denied.

#### Scenario: User confirms within timeout
- **WHEN** User confirms the request before timeout expires
- **THEN** Guardian SHALL return GuardianDecision::Allow

#### Scenario: User does not confirm within timeout
- **WHEN** Confirmation timeout expires without user response
- **THEN** Guardian SHALL return GuardianDecision::Deny with reason "Confirmation timeout"

---

### Requirement: GuardianDecision shall encode action type and blocking behavior

The GuardianDecision::NeedUserConfirm variant SHALL contain sufficient information for the caller to determine the confirmation strategy.

```rust
pub enum GuardianDecision {
    Allow,
    Deny { reason: String },
    NeedUserConfirm {
        request: ApprovalRequest,
        timeout: Duration,
        blocking: bool,
        action_type: ActionType,
    },
}
```

#### Scenario: NeedUserConfirm encodes blocking behavior
- **WHEN** GuardianDecision::NeedUserConfirm is constructed
- **THEN** decision SHALL include blocking=true for Shell, blocking=false for Network

#### Scenario: NeedUserConfirm encodes action type
- **WHEN** GuardianDecision::NeedUserConfirm is constructed
- **THEN** decision SHALL include action_type=Shell|Network|Credential for routing confirmation logic