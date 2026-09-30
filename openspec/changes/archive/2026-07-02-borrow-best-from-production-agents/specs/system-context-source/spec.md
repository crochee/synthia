## ADDED Requirements

### Requirement: SystemContext SHALL define Source trait with 5 functions

The `SystemContext` MUST define a `Source` trait with the following 5 functions: `key(&self) -> &str` (unique identifier), `load(&self) -> Result<Value>` (load current value), `baseline(&self) -> Value` (initial default value), `update(&self, prev: &Value) -> Result<Option<Value>>` (compute update, return None if unchanged), `removed(&self) -> bool` (whether source should be removed). Each Source MUST implement `PartialEq` for its Value type to support reconcile comparison.

#### Scenario: Source trait implementation for environment

- **WHEN** an `EnvironmentSource` is implemented for the `environment` key
- **THEN** `key()` returns `"environment"`
- **AND** `load()` reads current environment variables
- **AND** `baseline()` returns the initial environment snapshot
- **AND** `update()` returns `None` if environment unchanged, `Some(new_value)` if changed

#### Scenario: Source removed flag triggers cleanup

- **WHEN** a Source's `removed()` returns `true`
- **THEN** the SystemContext removes the source from its registry
- **AND** subsequent reconcile calls do not include this source

---

### Requirement: SystemContext SHALL persist Snapshot for each Source

The SystemContext MUST persist a `Snapshot` for each Source containing the encoded value and a revision counter. The Snapshot MUST be serializable (serde) and stored alongside session metadata. On session resume, Snapshots MUST be loaded to restore the SystemContext state.

#### Scenario: Snapshot persisted on source update

- **WHEN** an `EnvironmentSource` updates its value
- **THEN** a new `Snapshot` is created with the encoded value and incremented revision
- **AND** the Snapshot is persisted to session metadata

#### Scenario: Snapshot loaded on session resume

- **WHEN** a session is resumed
- **THEN** all `Snapshot`s from the previous session are loaded
- **AND** the SystemContext restores each Source's value from its Snapshot
- **AND** subsequent reconcile calls use the restored values as the previous state

---

### Requirement: SystemContext reconcile SHALL return 4 possible states

The `reconcile` function MUST compare the current Source value against the previous Snapshot and return one of 4 states: `Unchanged` (value identical, no action), `Updated` (value changed, system prompt should be regenerated), `ReplacementReady` (value changed and new snapshot is prepared for atomic swap), `ReplacementBlocked` (value changed but cannot be replaced, e.g., mid-turn). The comparison MUST use `PartialEq` on the Value type.

#### Scenario: Unchanged when value identical

- **WHEN** `reconcile` is called and `current_value == previous_snapshot.value`
- **THEN** `Unchanged` is returned
- **AND** no system prompt regeneration occurs
- **AND** no Snapshot update is persisted

#### Scenario: Updated when value changed outside turn

- **WHEN** `reconcile` is called between turns and `current_value != previous_snapshot.value`
- **THEN** `Updated` is returned
- **AND** the system prompt is regenerated with the new value
- **AND** a new Snapshot is persisted

#### Scenario: ReplacementReady during atomic swap

- **WHEN** `reconcile` is called during a turn and `current_value != previous_snapshot.value`
- **AND** the swap is safe (no in-flight tool calls depending on old value)
- **THEN** `ReplacementReady` is returned with the new Snapshot prepared
- **AND** the caller can perform the atomic swap

#### Scenario: ReplacementBlocked during in-flight tool call

- **WHEN** `reconcile` is called during a turn
- **AND** an in-flight tool call depends on the old system prompt value
- **THEN** `ReplacementBlocked` is returned
- **AND** the caller MUST defer the swap until the tool call completes
- **AND** a warning is logged

---

### Requirement: SystemContext SHALL NOT be exposed as a tool

The SystemContext and its Source management MUST NOT be exposed as an LLM-callable tool. SystemContext belongs to the system prompt layer, which is excluded from tool-ification per user decision Q3 ("system prompt, permission policy, session NOT as tool"). Sources are managed by the runtime and system events, not by LLM tool calls.

#### Scenario: No SystemContext tool registered

- **WHEN** the tool registry is queried
- **THEN** no tool named `update_system_context` or similar exists
- **AND** the LLM cannot modify SystemContext sources via tool calls

#### Scenario: Source updates triggered by runtime events only

- **WHEN** an environment variable changes
- **THEN** the runtime updates the `EnvironmentSource` directly
- **AND** no LLM tool call is involved
- **AND** the reconcile process runs in the background
