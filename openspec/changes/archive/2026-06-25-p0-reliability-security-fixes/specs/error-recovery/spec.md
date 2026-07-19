<!--
Delta spec for error-recovery capability.
-->

## ADDED Requirements

### Requirement: L5 Reset Scope Fallback

When `determine_scope` returns `ToolState` or `Full` (unimplemented scopes), the ResetCoordinator SHALL fall back to `Conversation` scope instead of failing, and emit a warning log indicating the fallback.

#### Scenario: ToolState scope fallback

- **WHEN** `consecutive_errors` is between 6 and 10
- **AND** `determine_scope` returns `ResetScope::ToolState`
- **THEN** the ResetCoordinator SHALL execute `Conversation` reset instead
- **AND** emit a warning log: "ToolState reset not implemented, falling back to Conversation"
- **AND** the reset SHALL succeed (not trigger cooldown)

#### Scenario: Full scope fallback

- **WHEN** `consecutive_errors` is 10 or more
- **AND** `determine_scope` returns `ResetScope::Full`
- **THEN** the ResetCoordinator SHALL execute `Conversation` reset instead
- **AND** emit a warning log: "Full reset not implemented, falling back to Conversation"
- **AND** the reset SHALL succeed (not trigger cooldown)

---

### Requirement: No Cooldown on Fallback Reset

When a fallback reset (ToolState/Full → Conversation) is executed, the ResetCoordinator SHALL NOT start the cooldown timer, since the reset succeeded.

#### Scenario: fallback reset does not trigger cooldown

- **WHEN** a fallback reset is executed (ToolState → Conversation)
- **AND** the Conversation reset succeeds
- **THEN** the cooldown timer SHALL NOT be started
- **AND** subsequent reset requests SHALL be accepted immediately
