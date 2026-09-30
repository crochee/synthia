## MODIFIED Requirements

### Requirement: Error recovery cooldown SHALL only trigger on terminal failure

The cooldown timestamp SHALL be stored only when `handle_error` returns `RecoveryResult::FailFast`. The cooldown SHALL NOT be entered when `handle_error` returns `RecoveryResult::Escalated` (an escalation attempt is not a terminal failure).

#### Scenario: Escalation does not enter cooldown
- **WHEN** `handle_error` is called and returns `RecoveryResult::Escalated(L2Retry)`
- **THEN** the cooldown timestamp SHALL NOT be stored

#### Scenario: FailFast enters cooldown
- **WHEN** `handle_error` is called and returns `RecoveryResult::FailFast("In cooldown period")`
- **THEN** the cooldown timestamp SHALL be stored

### Requirement: Successful recovery SHALL clear cooldown timestamp

`record_success()` SHALL clear the cooldown timestamp by storing0, so subsequent errors are not blocked by a stale cooldown from a previous failure.

#### Scenario: Success clears cooldown
- **WHEN** `record_success()` is called after a recovery succeeds
- **THEN** `last_recovery_time` SHALL be set to 0
- **AND** the next `handle_error` call within the cooldown window SHALL proceed normally

---

## ADDED Requirements

### Requirement: Cooldown test SHALL verify escalation-before-cooldown semantics

The test `test_coordinator_cooldown` SHALL verify that: (1) first `handle_error` call returns `Escalated` without entering cooldown; (2) second `handle_error` call within the cooldown window returns `Escalated` because no cooldown was entered; (3) after a `FailFast`, subsequent calls within cooldown window return `FailFast`.

#### Scenario: Two escalations without FailFast
- **WHEN** first `handle_error` returns `Escalated(L2Retry)` and second is called within 5 seconds
- **THEN** second call SHALL return `Escalated(L3Fallback)` (cooldown not entered on first call)

#### Scenario: FailFast then immediate retry
- **WHEN** `handle_error` returns `FailFast` and a subsequent call is made within 5 seconds
- **THEN** that call SHALL return `FailFast` immediately