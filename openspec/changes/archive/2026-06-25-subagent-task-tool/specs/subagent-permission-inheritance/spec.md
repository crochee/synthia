## ADDED Requirements

### Requirement: Subagent permissions SHALL inherit only `Deny` rules from the parent session

When constructing a subagent session, the system SHALL copy all `PermissionRule` entries whose `action` is `Deny` from the parent session into the subagent's permission set. `Allow` and `Ask` rules SHALL NOT be inherited.

#### Scenario: Parent has a deny rule for sensitive files
- **WHEN** the parent session has a `Deny` rule matching `*.env`
- **THEN** the subagent permission set SHALL also contain that `Deny` rule

#### Scenario: Parent has an allow rule for bash
- **WHEN** the parent session has an `Allow` rule for the `bash` tool
- **THEN** the subagent permission set SHALL NOT automatically inherit that `Allow` rule

---

### Requirement: Subagents SHALL default-deny the `task` tool unless explicitly allowed by their type

Unless the subagent type definition explicitly permits recursive task spawning, the derived subagent permission set SHALL contain a `Deny` rule for the `task` tool.

#### Scenario: General subagent type denies recursion
- **WHEN** a subagent of type `general` is spawned
- **THEN** its permission set SHALL contain `task: Deny`

#### Scenario: Custom subagent type allows recursion
- **WHEN** a subagent type registered via `RegisterAgent` explicitly sets `allow_task: true`
- **THEN** the `task` tool SHALL NOT be default-denied for that subagent

---

### Requirement: Subagents SHALL default-deny the `todowrite` tool unless explicitly allowed by their type

Unless the subagent type definition explicitly permits `todowrite`, the derived subagent permission set SHALL contain a `Deny` rule for the `todowrite` tool.

#### Scenario: Explore subagent type denies todo writes
- **WHEN** a subagent of type `explore` is spawned
- **THEN** its permission set SHALL contain `todowrite: Deny`

---

### Requirement: Subagent permission derivation SHALL be deterministic and testable

Given the same parent permission set and subagent type definition, `derive_subagent_permission` SHALL produce the same output every time.

#### Scenario: Derive permissions for the same inputs
- **WHEN** `derive_subagent_permission` is called twice with identical inputs
- **THEN** both outputs SHALL be identical
