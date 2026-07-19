# agent-tools-split Specification

## Purpose
TBD - created by archiving change compact-truncate-prune-convergence. Update Purpose after archive.
## Requirements
### Requirement: agent_tools MUST be split into 7 sub-files

The `agent_tools.rs` content MUST be moved into 7 separate files under `crates/synthia-agent/src/tools/agent/`:
- `mod.rs` — re-exports and `register_builtin_tools`
- `message_bus.rs` — `AgentMessage`, `MessageBus`, `SendError`, `ReceiveError`
- `instance.rs` — `AgentInstance`, lifecycle management
- `manager.rs` — `SubagentManager`, spawn coordination
- `tools/mod.rs` — re-exports
- `tools/spawn.rs` — `AgentTool` (`create_spawn_agent_tool`)
- `tools/send.rs` — `SendMessageTool`
- `tools/team_create.rs` — `TeamCreateTool`
- `tools/team_delete.rs` — `TeamDeleteTool`

#### Scenario: Each sub-file is < 300 lines
- **WHEN** the split is complete
- **THEN** each of the 7 sub-files SHALL contain fewer than 300 lines (excluding blank lines and pure comments)
- **AND** the largest sub-file SHALL be at most 300 lines

#### Scenario: Original file is preserved as shim
- **WHEN** the split is complete
- **THEN** the original `crates/synthia-agent/src/tools/agent_tools.rs` file SHALL exist as a shim
- **AND** the shim SHALL contain `pub use crate::tools::agent::*;` (or equivalent re-exports)
- **AND** the shim MUST NOT contain any business logic, only re-exports

### Requirement: Public API surface SHALL be 100% backward compatible

All public items previously accessible via `synthia_agent::tools::agent_tools::*` MUST remain accessible via the same path after the split. No external consumer (synthia-cli, synthia-server, or third-party code) SHALL need to change any `use` statement.

#### Scenario: agent_tools::AgentTool is still importable
- **WHEN** external code does `use synthia_agent::tools::agent_tools::AgentTool;`
- **THEN** the import SHALL resolve successfully
- **AND** `AgentTool` SHALL be the same type as before the split

#### Scenario: agent_tools::MessageBus is still importable
- **WHEN** external code does `use synthia_agent::tools::agent_tools::MessageBus;`
- **THEN** the import SHALL resolve successfully
- **AND** `MessageBus` SHALL be the same type as before the split

#### Scenario: register_builtin_tools is still callable
- **WHEN** external code calls `synthia_agent::tools::agent_tools::register_builtin_tools(...)`
- **THEN** the function SHALL be callable with the same arguments
- **AND** SHALL produce the same tool registration effects as before the split

### Requirement: Zero behavior change MUST be verified by all existing tests passing

After the split, the entire workspace test suite (`cargo test --workspace --all-features`) MUST pass without any test modification. No test file shall be edited as part of the split. If a test fails, the split is incomplete and MUST be reverted.

#### Scenario: All existing unit tests pass
- **WHEN** `cargo test --workspace --all-features` is run after the split
- **THEN** all existing unit tests SHALL pass
- **AND** the test count SHALL match the pre-split count exactly

#### Scenario: All existing integration tests pass
- **WHEN** `cargo test --workspace --all-features --tests` is run after the split
- **THEN** all existing integration tests SHALL pass
- **AND** no test file SHALL have been modified during the split

#### Scenario: No new clippy warnings
- **WHEN** `cargo clippy --all-targets --all-features --tests --all` is run after the split
- **THEN** the number of clippy warnings SHALL be ≤ the pre-split count
- **AND** no new clippy warnings SHALL be introduced by the split

#### Scenario: agent_tools.rs file size is minimal
- **WHEN** the split is complete
- **THEN** the shim `agent_tools.rs` file SHALL contain fewer than 10 lines of code (excluding the license/doc header)
- **AND** the only content SHALL be re-exports

