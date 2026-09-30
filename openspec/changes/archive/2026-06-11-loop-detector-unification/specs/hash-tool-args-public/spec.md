# hash-tool-args-public Specification

## Purpose

Promote `hash_tool_args` to a public function in `synthia_guardian` so that callers (e.g. the agent) can compute a stable `(tool_id, args_hash)` key without duplicating the implementation.

## ADDED Requirements

### Requirement: hash_tool_args shall be a public function in synthia-guardian

The `hash_tool_args` function SHALL be publicly exported from `synthia_guardant::hash_tool_args` (re-exported via `synthia_guardian::prelude` or direct module path). The function SHALL hash a tool name and args JSON into a `(u64, u64)` pair for use as a stable, allocation-free key.

#### Scenario: Public API
- **WHEN** a caller imports `hash_tool_args` from `synthia_guardian`
- **THEN** the function SHALL have the signature: `pub fn hash_tool_args(tool_name: &str, args_json: &str) -> (u64, u64)`
- **AND** the first element of the tuple SHALL be the hash of `tool_name` alone
- **AND** the second element SHALL be the hash of `args_json` alone
- **AND** the function SHALL be `#[must_use]`

#### Scenario: Allocation-free implementation
- **WHEN** `hash_tool_args` is called
- **THEN** it SHALL NOT allocate any heap memory
- **AND** it SHALL use a stack-allocated `DefaultHasher` (or `AHasher`)
- **AND** it SHALL complete in O(len(tool_name) + len(args_json)) time

---

### Requirement: hash_tool_args shall produce deterministic, distinct hashes

The function SHALL produce deterministic output for the same input, and SHALL distinguish between different inputs along both dimensions (tool name and args).

#### Scenario: Deterministic
- **WHEN** the same `(tool_name, args_json)` is hashed twice
- **THEN** both calls SHALL return the same `(u64, u64)` pair

#### Scenario: Different tool name produces different tool_id
- **WHEN** two calls have the same `args_json` but different `tool_name`
- **THEN** the first elements of the tuples SHALL differ
- **AND** the second elements SHALL match

#### Scenario: Different args produce different args_hash
- **WHEN** two calls have the same `tool_name` but different `args_json`
- **THEN** the first elements of the tuples SHALL match
- **AND** the second elements SHALL differ

---

### Requirement: hash_tool_args shall be the only canonical implementation

There SHALL be exactly one definition of `hash_tool_args` in the Synthia workspace, located in `synthia_guardian`. Other crates (e.g. `synthia-agent`) MUST NOT define their own `hash_tool_args` function.

#### Scenario: Single source of truth
- **WHEN** `cargo build --workspace` is run
- **THEN** exactly one definition of `hash_tool_args` SHALL be compiled
- **AND** it SHALL be in `crates/synthia-guardian/src/loop_detector.rs` (or a dedicated hash module)

#### Scenario: Removal of duplicate agent implementation
- **WHEN** `synthia-agent` is rebuilt after the migration
- **THEN** the function `hash_tool_args` SHALL NOT be defined in `crates/synthia-agent/src/stream_builder/loop_detection.rs`
- **AND** any caller in `synthia-agent` that previously used the local version SHALL now use `synthia_guardian::hash_tool_args`
