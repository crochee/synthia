# convergent-prompt-assembly Specification

## ADDED Requirements

### Requirement: ContextAssembler SHALL be the only public prompt-assembly entry point

All prompt assembly paths in the workspace SHALL flow through `synthia_context::assembler::ContextAssembler`. No other crate SHALL expose a public API that constructs `Vec<ChatMessage>` directly without going through `ContextAssembler::assemble()`.

#### Scenario: StreamBuilder uses ContextAssembler
- **WHEN** `StreamBuilder::run` builds a prompt for an LLM call
- **THEN** it SHALL call `ContextAssembler::assemble(budget)` exclusively
- **THEN** it SHALL NOT call any `ContextBuilder` private struct, `prompt::builder`, or `system_context` directly

#### Scenario: AgentBuilder delegates to ContextAssembler
- **WHEN** `AgentBuilder` is asked to construct a prompt
- **THEN** it SHALL delegate to `ContextAssembler` rather than maintaining its own assembly logic

#### Scenario: System context composition uses ContextAssembler
- **WHEN** `synthia_context::system_context` is asked for system prompt
- **THEN** it SHALL be reimplemented as a thin wrapper around `ContextAssembler::assemble_system()` or removed entirely

### Requirement: Agent private ContextBuilder SHALL be removed

The private `agent::stream_builder::context_builder::ContextBuilder` struct SHALL be removed. All callers SHALL use `ContextAssembler`.

#### Scenario: ContextBuilder struct not present
- **WHEN** searching the workspace for `struct ContextBuilder`
- **THEN** no struct named `ContextBuilder` SHALL be defined in `synthia-agent`
- **THEN** the file `crates/synthia-agent/src/stream_builder/context_builder.rs` SHALL be deleted entirely (no re-exports — the public `ContextAssembler` in `synthia-context` is the canonical location)

#### Scenario: All call sites migrated
- **WHEN** searching for `.build()` / `.with_section()` calls on the removed `ContextBuilder`
- **THEN** no such calls SHALL exist in `synthia-agent` source
- **THEN** all former call sites SHALL use `ContextAssembler` instead

### Requirement: ContextAssembler SHALL expose section-by-name lookup

`ContextAssembler` SHALL expose a public method `section_by_name(&self, name: &str) -> Option<&Section>` to enable post-assembly inspection (used by `StreamBuilder` self-reflection to read specific system sections).

#### Scenario: Lookup existing section
- **WHEN** `assembler.section_by_name("system_prompt")` is called
- **AND** a section with that name was added via `with_section`
- **THEN** it SHALL return `Some(&Section)`

#### Scenario: Lookup missing section
- **WHEN** `assembler.section_by_name("nonexistent")` is called
- **THEN** it SHALL return `None`

#### Scenario: Lookup is read-only
- **WHEN** `section_by_name` returns a reference
- **THEN** the returned `&Section` SHALL be `Send + Sync` (immutable borrow)
- **THEN** mutation SHALL NOT be possible through the returned reference

### Requirement: ContextAssembler SHALL expose system prompt bytes for prefix tracking

`ContextAssembler` SHALL expose a `system_snapshot(&self) -> Vec<u8>` method returning the byte-level snapshot of the immutable system prefix. This snapshot is used by `PrefixTracker` to detect cache-affecting changes.

#### Scenario: Snapshot is deterministic
- **WHEN** `system_snapshot` is called twice on the same assembler state
- **THEN** both calls SHALL return byte-identical `Vec<u8>`

#### Scenario: Snapshot reflects added sections
- **WHEN** a new section is added via `with_section("foo", content)`
- **THEN** a subsequent `system_snapshot` call SHALL return bytes including `"foo"` content

#### Scenario: Snapshot is Send-safe
- **WHEN** `system_snapshot` is called from a `Send` context
- **THEN** the returned `Vec<u8>` SHALL be `Send` (no internal references to non-Send state)

### Requirement: Prompt assembly SHALL produce stable prefix across LLM calls

When the same `ContextAssembler` configuration is used across multiple LLM calls within a session (no new sections added, no section mutated), the byte-level prefix of the assembled prompt SHALL be identical.

#### Scenario: Stable prefix without section changes
- **WHEN** two consecutive LLM calls are made with the same `ContextAssembler` state
- **THEN** the first N bytes of the assembled prompts (where N = length of system section block) SHALL be byte-identical

#### Scenario: Prefix changes when section added
- **WHEN** a section is added via `with_section` between two LLM calls
- **THEN** the byte-level prefix SHALL differ
- **THEN** `PrefixTracker` SHALL record this as a prefix change

### Requirement: Migration SHALL be backward compatible

The migration from 5 prompt-assembly paths to 1 SHALL NOT change the public API of any external caller that already uses `ContextAssembler`.

#### Scenario: Existing ContextAssembler callers unaffected
- **WHEN** external code calls `ContextAssembler::new()`, `with_section()`, or `assemble()`
- **THEN** behavior SHALL be identical to pre-migration
- **THEN** function signatures SHALL be unchanged

#### Scenario: Internal callers migrated
- **WHEN** all internal call sites in `synthia-agent` are migrated to use `ContextAssembler`
- **THEN** `cargo test --workspace` SHALL pass
- **THEN** `cargo clippy --all-targets` SHALL report no new warnings
