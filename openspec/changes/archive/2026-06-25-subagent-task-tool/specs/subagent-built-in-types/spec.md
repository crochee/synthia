## ADDED Requirements

### Requirement: The system SHALL provide a built-in `general` subagent type

The `general` subagent type SHALL be advertised in the `task` tool description and SHALL have broad tool access suitable for multi-step research and implementation tasks. It SHALL default-deny `task` and `todowrite`.

#### Scenario: LLM requests a general subagent
- **WHEN** the LLM calls `task` with `subagent_type: "general"`
- **THEN** the system SHALL spawn a subagent with the `general` tool set and permissions

#### Scenario: General subagent attempts recursion
- **WHEN** a `general` subagent tries to call `task`
- **THEN** the call SHALL be denied by the permission system

---

### Requirement: The system SHALL provide a built-in `explore` subagent type

The `explore` subagent type SHALL be advertised in the `task` tool description and SHALL be restricted to read-only tools (e.g., `read`, `glob`, `grep`, `web_fetch`). It SHALL deny all write tools, `bash`, `task`, and `todowrite`.

#### Scenario: LLM requests an explore subagent
- **WHEN** the LLM calls `task` with `subagent_type: "explore"`
- **THEN** the system SHALL spawn a subagent with read-only tool access

#### Scenario: Explore subagent attempts a write
- **WHEN** an `explore` subagent tries to call `write`
- **THEN** the call SHALL be denied by the permission system

---

### Requirement: The `task` tool description SHALL dynamically list available subagent types

The description string exposed to the LLM for the `task` tool SHALL include the list of currently available subagent types, including built-in types and any custom types registered via `RegisterAgent`.

#### Scenario: Built-in types are available
- **WHEN** the tool registry builds the `task` tool description
- **THEN** the description SHALL list `general` and `explore`

#### Scenario: Custom type is registered
- **WHEN** a custom subagent type is registered at runtime
- **THEN** the `task` tool description SHALL include that type on subsequent registrations

---

### Requirement: Built-in subagent types SHALL have stable, documented identifiers

The identifiers `general` and `explore` SHALL be reserved. Custom types registered via `RegisterAgent` SHALL NOT be allowed to use these reserved identifiers.

#### Scenario: RegisterAgent tries to override built-in type
- **WHEN** `RegisterAgent` is called with a type name matching a built-in identifier
- **THEN** the system SHALL reject the registration
