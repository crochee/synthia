## ADDED Requirements

### Requirement: CompositeSandboxManager SHALL provide a prioritized fallback chain
The system SHALL provide a `CompositeSandboxManager` that holds an ordered list of `SandboxManager` backends and selects the first backend that returns a usable `SandboxAttempt`.

#### Scenario: Bubblewrap available
- **WHEN** `CompositeSandboxManager` is configured with `BubblewrapBackend` first and `LandlockBackend` second, and bubblewrap is available
- **THEN** `select` SHALL return `SandboxAttempt::Bubblewrap`.

#### Scenario: Bubblewrap unavailable but Landlock available
- **WHEN** bubblewrap is unavailable and Landlock is available
- **THEN** `select` SHALL return `SandboxAttempt::Landlock`.

#### Scenario: All backends unavailable
- **WHEN** neither bubblewrap nor Landlock is available
- **THEN** `select` SHALL return `SandboxAttempt::Unavailable`.

---

### Requirement: CompositeSandboxManager SHALL preserve fail-closed semantics
If every backend in the chain reports `Unavailable`, the composite manager SHALL return `SandboxAttempt::Unavailable` and SHALL NOT silently downgrade to `SandboxAttempt::None`.

#### Scenario: Empty backend chain with Standard policy
- **WHEN** `CompositeSandboxManager` has no backends and `SandboxPolicy::Standard` is requested
- **THEN** `select` SHALL return `SandboxAttempt::Unavailable`.

---

### Requirement: CompositeSandboxManager SHALL treat SandboxPolicy::None as no sandboxing
The composite manager SHALL short-circuit `SandboxPolicy::None` to `SandboxAttempt::None` without querying any backend.

#### Scenario: None policy requested
- **WHEN** `CompositeSandboxManager::select` is called with `SandboxPolicy::None`
- **THEN** it SHALL return `SandboxAttempt::None` immediately.

---
