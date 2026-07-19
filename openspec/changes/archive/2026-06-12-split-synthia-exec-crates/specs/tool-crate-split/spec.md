# tool-crate-split Specification (delta)

## ADDED Requirements

### Requirement: synthia-exec SHALL be split into synthia-tool-bash + synthia-tool-exec-base

The `synthia-exec` crate SHALL be split into two new workspace members:

- `synthia-tool-bash` — contains the four user-facing tool files (`bash_tool.rs`, `command_blacklist.rs`, `command_manager.rs`, `monitor.rs`) and the `tests/bash_utf8_panic.rs` integration test. Depends on `synthia-core`, `tokio`, `serde`, `serde_json`, `thiserror`, `tracing`, `chrono`, `uuid`, `parking_lot`.
- `synthia-tool-exec-base` — contains the generic async task executor (`exec/` submodule: `executor.rs`, `executor_types.rs`, `priority.rs`, `validation.rs`, `mod.rs`). Depends on `synthia-core`, `tokio`, `serde`, `serde_json`, `thiserror`, `tracing`, `uuid`, `priority-queue`, `parking_lot`, `toml`, and optionally `jsonschema`.

The two halves have **zero inter-crate dependencies** in production code (only the `BashTool` uses `CommandBlacklist` and `CommandManager`; those files are siblings within `synthia-tool-bash`).

The `synthia-exec` crate SHALL remain in the workspace as a 1-line shim that re-exports the two new crates' top-level symbols, so any external path of the form `synthia_exec::*` continues to resolve.

#### Scenario: Two new crates appear in the workspace
- **WHEN** the workspace's `Cargo.toml` `[workspace]` members are listed
- **THEN** `crates/synthia-tool-bash` SHALL be present
- **THEN** `crates/synthia-tool-exec-base` SHALL be present
- **THEN** `crates/synthia-exec` SHALL still be present (as a shim)
- **THEN** `cargo metadata --no-deps --format-version 1 | jq '.workspace_members'` SHALL list all three

#### Scenario: synthia-tool-bash contains the bash tool
- **WHEN** `crates/synthia-tool-bash/src/lib.rs` is read
- **THEN** it SHALL `pub use` `BashTool`, `CommandBlacklist`, `CommandManager`, and `MonitorTool`
- **THEN** the `bash_tool.rs` source file SHALL exist at `crates/synthia-tool-bash/src/bash_tool.rs`
- **THEN** the integration test SHALL be at `crates/synthia-tool-bash/tests/bash_utf8_panic.rs` and import from `synthia_tool_bash::*`

#### Scenario: synthia-tool-exec-base contains the executor
- **WHEN** `crates/synthia-tool-exec-base/src/lib.rs` is read
- **THEN** it SHALL `pub mod exec` and `pub use exec::validate_parameters`
- **THEN** the five `exec/*.rs` files SHALL be present at `crates/synthia-tool-exec-base/src/exec/`
- **THEN** `Executor`, `TaskPriority`, `ExecutorConfig`, `ResourceUsage`, `TaskError`, `TaskHandle` SHALL be reachable as `synthia_tool_exec_base::Executor` etc.

#### Scenario: synthia-exec shim preserves public API
- **WHEN** downstream code (or the shim's own tests) imports `synthia_exec::bash_tool::BashTool` or `synthia_exec::exec::Executor`
- **THEN** the import SHALL resolve through the shim
- **THEN** the shim's `lib.rs` SHALL consist of `pub use synthia_tool_bash::*;` and `pub use synthia_tool_exec_base::*;` and no other code

#### Scenario: Internal use crate::* paths are unaffected
- **WHEN** the moved source files are inspected
- **THEN** `bash_tool.rs` SHALL still use `use crate::{command_blacklist::CommandBlacklist, command_manager::CommandManager};`
- **THEN** `monitor.rs` SHALL still use `use crate::command_manager::CommandManager;`
- **THEN** `exec/executor.rs` SHALL still use `use crate::exec::{...}` for the executor's internal sibling types
- **THEN** none of the moved files SHALL cross crate boundaries with `use crate::...` references

### Requirement: No behavioral changes to public types

The split is a code reorganization only. The behavior, signatures, and semantics of every public type and function in the split SHALL be preserved bit-for-bit:

- `BashTool::new`, `BashTool::call` and its parameter schema
- `CommandBlacklist::new`, `is_command_blacklisted`, `is_command_allowed`, `BLACKLISTED_PATTERNS`
- `CommandManager::new`, `register`, `get_child`, `list`, `remove`
- `MonitorTool::new`, `MonitorTool::call`
- `Executor::new`, `Executor::submit`, `Executor::shutdown`, etc.
- `validate_parameters`

#### Scenario: Public types retain their methods and field shapes
- **WHEN** any test that previously called `BashTool::new(...)` or `CommandBlacklist::is_command_blacklisted("rm -rf /")` is re-run against the new crates
- **THEN** the test SHALL pass without source modifications other than the `use` path
- **THEN** the returned values SHALL be byte-identical to the pre-split outputs

#### Scenario: All existing tests pass after the split
- **WHEN** `cargo test -p synthia-tool-bash -p synthia-tool-exec-base -p synthia-exec` is run
- **THEN** the `bash_utf8_panic` integration test SHALL pass
- **THEN** all `exec::validation` and `exec::executor` unit tests SHALL pass
- **THEN** zero regressions SHALL be observed in the bash tool's own unit tests

### Requirement: synthia-tool-bash and synthia-tool-exec-base SHALL NOT depend on synthia-exec

To prevent a dependency cycle (synthia-exec → synthia-tool-bash → synthia-exec), the two new crates SHALL NOT have any path-dependency on `synthia-exec`. They depend on `synthia-core` and selected third-party crates only.

#### Scenario: synthia-tool-bash has no synthia-exec dependency
- **WHEN** `crates/synthia-tool-bash/Cargo.toml` is read
- **THEN** it SHALL NOT contain `synthia-exec = { path = "../synthia-exec" }` (or any other reference)
- **THEN** its only `synthia-*` path dependency SHALL be `synthia-core`

#### Scenario: synthia-tool-exec-base has no synthia-exec dependency
- **WHEN** `crates/synthia-tool-exec-base/Cargo.toml` is read
- **THEN** it SHALL NOT contain any reference to `synthia-exec`
- **THEN** its only `synthia-*` path dependency SHALL be `synthia-core`
