<!--
Delta spec for unify-token-usage-types.
Capability: unified-token-usage (new)
-->

## ADDED Requirements

### Requirement: synthia-provider SHALL expose TokenUsage as the canonical type

`crates/synthia-provider/src/types.rs` SHALL define a `pub struct TokenUsage` as the canonical token usage data type across all crates. The struct SHALL have exactly four fields:
- `prompt_tokens: usize`
- `completion_tokens: usize`
- `total_tokens: usize`
- `cached_prompt_tokens: Option<usize>`

The struct SHALL derive `Clone, Debug, Default, Serialize, Deserialize`. The `cached_prompt_tokens` field SHALL have `#[serde(default)]` so older JSON payloads missing the field deserialize as `None`.

#### Scenario: Canonical type has four fields with serde derives
- **WHEN** `crates/synthia-provider/src/types.rs:401-406` is read
- **THEN** a `pub struct TokenUsage` SHALL be defined with the four fields above
- **THEN** the struct SHALL derive `Clone, Debug, Default, Serialize, Deserialize`
- **THEN** the `cached_prompt_tokens` field SHALL be annotated with `#[serde(default)]`

#### Scenario: Default values match zero-state
- **WHEN** `TokenUsage::default()` is called
- **THEN** `prompt_tokens` SHALL be `0`
- **THEN** `completion_tokens` SHALL be `0`
- **THEN** `total_tokens` SHALL be `0`
- **THEN** `cached_prompt_tokens` SHALL be `None`

#### Scenario: Old JSON without cached_prompt_tokens deserializes
- **WHEN** a JSON string `{"prompt_tokens":10,"completion_tokens":20,"total_tokens":30}` is deserialized as `TokenUsage`
- **THEN** deserialization SHALL succeed (because `#[serde(default)]` supplies `None` for the missing field)
- **THEN** `cached_prompt_tokens` SHALL equal `None`

### Requirement: synthia-session SHALL re-export TokenUsage via 1-line shim

`crates/synthia-session/src/types.rs` SHALL NOT define a new `TokenUsage` struct. Instead, it SHALL contain a single line `pub use synthia_provider::types::TokenUsage;` so that all existing call sites referencing `synthia_session::TokenUsage` (or `synthia_session::types::TokenUsage`) continue to compile and resolve to the canonical type.

#### Scenario: Session types module re-exports
- **WHEN** `crates/synthia-session/src/types.rs` is read
- **THEN** the local `pub struct TokenUsage { ... }` definition SHALL NOT be present
- **THEN** a line `pub use synthia_provider::types::TokenUsage;` SHALL be present

#### Scenario: External users of synthia_session::TokenUsage compile
- **WHEN** an external crate writes `use synthia_session::TokenUsage;`
- **THEN** the import SHALL resolve to `synthia_provider::types::TokenUsage`
- **THEN** the imported type SHALL be `Serialize + Deserialize + Default + Clone + Debug`

### Requirement: synthia-agent SHALL re-export TokenUsage via 1-line shim

`crates/synthia-agent/src/events.rs` SHALL NOT define a new `TokenUsage` struct. It SHALL contain a single line `pub use synthia_provider::types::TokenUsage;` so that all existing call sites referencing `synthia_agent::events::TokenUsage` (or `synthia_agent::types::TokenUsage` via the existing `lib.rs` re-export) continue to compile and resolve to the canonical type.

#### Scenario: Agent events module re-exports
- **WHEN** `crates/synthia-agent/src/events.rs` is read
- **THEN** the local `pub struct TokenUsage { ... }` definition SHALL NOT be present
- **THEN** a line `pub use synthia_provider::types::TokenUsage;` SHALL be present

#### Scenario: crate::events::TokenUsage construction sites still compile
- **WHEN** code at `crates/synthia-agent/src/stream_builder/builder.rs:413` constructs a value using `crate::events::TokenUsage { ... }` syntax
- **THEN** the constructor call SHALL continue to compile
- **THEN** the resulting value SHALL be the canonical 4-field `TokenUsage`

### Requirement: synthia-context SHALL remove TokenUsageSnapshot and use canonical TokenUsage

`crates/synthia-context/src/checkpoint.rs` SHALL NOT define a `TokenUsageSnapshot` struct. All internal references that previously pointed to `TokenUsageSnapshot` SHALL be replaced with `synthia_provider::types::TokenUsage`. The `Checkpoint` struct that previously held `pub token_usage: TokenUsageSnapshot` SHALL hold `pub token_usage: synthia_provider::types::TokenUsage` instead.

#### Scenario: TokenUsageSnapshot type is removed
- **WHEN** `crates/synthia-context/src/checkpoint.rs` is read
- **THEN** no `pub struct TokenUsageSnapshot` SHALL be defined
- **THEN** a `grep -r "TokenUsageSnapshot" crates/` SHALL return zero matches in source files

#### Scenario: Checkpoint.token_usage is canonical type
- **WHEN** the `Checkpoint` struct in `crates/synthia-context/src/checkpoint.rs:44-58` is read
- **THEN** its `token_usage` field SHALL be of type `synthia_provider::types::TokenUsage`
- **THEN** the field SHALL carry the four canonical fields (including `cached_prompt_tokens`)

#### Scenario: Context checkpoint serialization roundtrip succeeds
- **WHEN** a `Checkpoint` is serialized to JSON and deserialized back
- **THEN** the deserialized `token_usage` SHALL contain the same `prompt_tokens`, `completion_tokens`, `total_tokens`, and `cached_prompt_tokens` values

### Requirement: No crate outside synthia-provider SHALL define a TokenUsage struct

After this change, the only `pub struct TokenUsage` definition in the workspace SHALL be the canonical one in `crates/synthia-provider/src/types.rs`. The other three crates (synthia-session, synthia-agent, synthia-context) SHALL re-export or reference it.

#### Scenario: Single TokenUsage struct definition
- **WHEN** `grep -rn "pub struct TokenUsage" crates/` is run
- **THEN** exactly one match SHALL appear at `crates/synthia-provider/src/types.rs`
- **THEN** no other crate SHALL contain `pub struct TokenUsage` or `pub struct TokenUsageSnapshot`

#### Scenario: No remaining TokenUsageSnapshot references
- **WHEN** `grep -rn "TokenUsageSnapshot" crates/` is run
- **THEN** zero matches SHALL appear in any source file (lib, bin, test, example)

### Requirement: Backward compatibility for serialized checkpoint JSON

A JSON file written by a previous version (3-field `TokenUsage` without `cached_prompt_tokens`) SHALL still deserialize successfully into the new 4-field canonical `TokenUsage` type.

#### Scenario: Legacy 3-field JSON deserializes to 4-field struct
- **WHEN** a legacy JSON payload `{"prompt_tokens":100,"completion_tokens":50,"total_tokens":150}` is deserialized as `synthia_provider::types::TokenUsage`
- **THEN** deserialization SHALL succeed without error
- **THEN** `cached_prompt_tokens` SHALL equal `None` (supplied by `#[serde(default)]`)

#### Scenario: New 4-field JSON deserializes correctly
- **WHEN** a new JSON payload `{"prompt_tokens":100,"completion_tokens":50,"total_tokens":150,"cached_prompt_tokens":80}` is deserialized
- **THEN** all four fields SHALL be populated with the expected values
</content>
</invoke>