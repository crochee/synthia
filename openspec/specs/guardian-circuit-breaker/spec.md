# guardian-circuit-breaker Specification

## Purpose
TBD - created by archiving change gap-01-guardian-enhancement. Update Purpose after archive.
## Requirements
### Requirement: Circuit breaker shall track Guardian denial patterns

The GuardianCircuitBreaker SHALL track denial patterns per session to detect when a session requires user intervention. Tracking SHALL be persistent across the session lifetime.

The circuit breaker SHALL maintain:
- consecutive_denials: count of consecutive Guardian denials
- total_denials: total Guardian denials in session lifetime
- session_interrupt: boolean flag indicating if interrupt has been triggered

#### Scenario: Track consecutive denials
- **WHEN** Guardian denies a request
- **THEN** consecutive_denials SHALL be incremented and total_denials SHALL be incremented

#### Scenario: Reset consecutive denials on approval
- **WHEN** Guardian approves a request
- **THEN** consecutive_denials SHALL be reset to 0

#### Scenario: Track total denials
- **WHEN** Guardian denies a request
- **THEN** total_denials SHALL be incremented by 1

---

### Requirement: Circuit breaker shall trigger interrupt on threshold

The GuardianCircuitBreaker SHALL trigger session interrupt when either threshold is reached:
- 3 consecutive denials, OR
- 10 total denials

Once triggered, session_interrupt SHALL remain true for the remainder of the session.

#### Scenario: Trigger interrupt after 3 consecutive denials
- **WHEN** consecutive_denials reaches 3
- **THEN** session_interrupt SHALL be set to true

#### Scenario: Trigger interrupt after 10 total denials
- **WHEN** total_denials reaches 10
- **THEN** session_interrupt SHALL be set to true

#### Scenario: Interrupt persists for session
- **WHEN** session_interrupt is true
- **THEN** session_interrupt SHALL remain true even if subsequent approvals occur

---

### Requirement: Circuit breaker state shall be accessible for session management

The GuardianCircuitBreaker SHALL expose methods to query current state and record decisions.

```rust
impl GuardianCircuitBreaker {
    pub fn record_denial(&mut self);
    pub fn record_approval(&mut self);
    pub fn should_interrupt(&self) -> bool;
    pub fn reset(&mut self);
}
```

#### Scenario: Query interrupt state
- **WHEN** should_interrupt() is called
- **THEN** Guardian SHALL return true if session should be interrupted, false otherwise

#### Scenario: Reset circuit breaker
- **WHEN** reset() is called
- **THEN** consecutive_denials and total_denials SHALL be set to 0, session_interrupt SHALL be set to false

