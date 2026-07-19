# Proposal: split `synthia-exec` into `synthia-tool-bash` + `synthia-tool-exec-base`

## Why

`synthia-exec` is a single 955-line crate that bundles two unrelated concerns:

1. **A user-facing Bash tool** — `BashTool`, `CommandBlacklist`, `CommandManager`, `MonitorTool`.
   These are concrete tool implementations that the agent invokes.
2. **A generic async task executor** — `Executor`, `TaskPriority`, `ResourceUsage`,
   `validate_parameters`. This is a self-contained priority-queue + semaphore-based
   task scheduler with no bash-specific code.

The two halves have **zero production dependencies between them** today: `bash_tool.rs`
only imports `command_blacklist` and `command_manager` (sibling files), and the
`exec/` module is not imported by any of the tool files. The crate is a flat
namespace that confuses the build graph and prevents either concern from
evolving independently.

Splitting the crate removes the false coupling and aligns with the
existing `synthia-tool` crate (which already provides the `Tool` trait,
`ToolRegistry`, and types): `synthia-tool-bash` becomes a *concrete tool
implementation* under the same `synthia-tool-*` naming family, and
`synthia-tool-exec-base` becomes a standalone execution primitive that
future changes (Turn model, ACP) can build on.

## What Changes

- **NEW `crates/synthia-tool-bash/`** containing the four tool files
  (`bash_tool.rs`, `command_blacklist.rs`, `command_manager.rs`, `monitor.rs`)
  and the `tests/bash_utf8_panic.rs` integration test.
- **NEW `crates/synthia-tool-exec-base/`** containing the `exec/` submodule
  (`executor.rs`, `executor_types.rs`, `priority.rs`, `validation.rs`, `mod.rs`).
- **MOVE** `synthia-exec/tests/bash_utf8_panic.rs` into
  `synthia-tool-bash/tests/` and update its import path.
- **KEEP** `synthia-exec` as a 1-line shim (`pub use synthia_tool_bash::*; pub use synthia_tool_exec_base::*;`)
  per project memory rule "Module split pattern: keep original file as
  `pub use sub_module::*` shim, never delete the original path".
- **WORKSPACE**: replace `crates/synthia-exec` entry with
  `crates/synthia-tool-bash` and `crates/synthia-tool-exec-base` in
  `Cargo.toml`.
- **DEPENDENCIES**:
  - `synthia-tool-bash` depends on `synthia-core` (no `synthia-exec`).
  - `synthia-tool-exec-base` depends on `synthia-core` (no `synthia-exec`).
  - The shim `synthia-exec` re-exports from both new crates and otherwise
    has no body of its own.

## Impact

| Item | Before | After |
|------|--------|-------|
| Workspace crates | 22 | 23 (+1 net; -1 + 2) |
| `synthia-exec` LOC | 955 | ~5 (shim only) |
| Cross-crate deps in split | 0 | 2 thin crates (sibling) |
| Test moves | 0 | 1 (bash_utf8_panic.rs) |
| Public API breaks | n/a | 0 (shim preserves the path) |
| Compile-time wins | — | Slight (smaller unit graphs) |

## Affected specs

None directly — this is a code reorganization, not a behavioral change.
The existing `command-blacklist` spec is unaffected because the type's
public API and location (as `synthia_exec::command_blacklist::CommandBlacklist`)
are preserved through the shim.

## Out of scope

- Implementing the `Executor` integration into `BashTool` (separate change).
- Removing the `Executor` if it remains unused after the split (separate
  follow-up; some of its internal tests are valuable documentation of the
  scheduler contract).
- Renaming the shim or changing `synthia-exec`'s version.
