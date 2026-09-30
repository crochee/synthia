# Design: split `synthia-exec` into `synthia-tool-bash` + `synthia-tool-exec-base`

## 1. Crate boundary

```
                          ┌──────────────────────────────┐
                          │  synthia-exec (shim)         │
                          │  pub use synthia_tool_bash::*│
                          │  pub use synthia_tool_exec_*│
                          └────────────┬─────────────────┘
                                       │ (re-exports)
                ┌──────────────────────┴────────────────────┐
                ▼                                            ▼
  ┌─────────────────────────────┐         ┌──────────────────────────────┐
  │ synthia-tool-bash           │         │ synthia-tool-exec-base       │
  │                             │         │                              │
  │ - bash_tool.rs              │         │ - exec/executor.rs           │
  │ - command_blacklist.rs      │         │ - exec/executor_types.rs     │
  │ - command_manager.rs        │         │ - exec/priority.rs           │
  │ - monitor.rs                │         │ - exec/validation.rs         │
  │ - tests/bash_utf8_panic.rs  │         │ - exec/mod.rs                │
  │                             │         │                              │
  │ depends on:                 │         │ depends on:                  │
  │   synthia-core              │         │   synthia-core               │
  │   tokio, serde, etc.        │         │   tokio, parking_lot,        │
  │                             │         │   priority-queue, etc.       │
  └─────────────────────────────┘         └──────────────────────────────┘
```

## 2. Module naming (Rust path stability)

### 2.1 `synthia-tool-bash`

| Old path (`synthia-exec`) | New path (`synthia-tool-bash`) | Shim re-export? |
|---------------------------|--------------------------------|-----------------|
| `synthia_exec::bash_tool::BashTool` | `synthia_tool_bash::BashTool` | Yes |
| `synthia_exec::command_blacklist::CommandBlacklist` | `synthia_tool_bash::command_blacklist::CommandBlacklist` | Yes |
| `synthia_exec::command_manager::CommandManager` | `synthia_tool_bash::command_manager::CommandManager` | Yes |
| `synthia_exec::monitor::MonitorTool` | `synthia_tool_bash::MonitorTool` | Yes |
| `synthia_exec::validate_parameters` | n/a (not bash-specific) | Re-exported from `synthia-tool-exec-base` |

### 2.2 `synthia-tool-exec-base`

| Old path (`synthia-exec`) | New path (`synthia-tool-exec-base`) | Shim re-export? |
|---------------------------|--------------------------------------|-----------------|
| `synthia_exec::exec::Executor` | `synthia_tool_exec_base::Executor` | Yes |
| `synthia_exec::exec::TaskPriority` | `synthia_tool_exec_base::TaskPriority` | Yes |
| `synthia_exec::exec::validate_parameters` | `synthia_tool_exec_base::validate_parameters` | Yes |
| `synthia_exec::exec::executor_types::*` | `synthia_tool_exec_base::executor_types::*` | Yes |

### 2.3 Shim semantics

The `synthia-exec` crate's `lib.rs` is reduced to:

```rust
// Compatibility shim — split into synthia-tool-bash + synthia-tool-exec-base.
// New code should depend on the two crates directly; this shim exists so
// existing dependents (none in this workspace as of the split) and external
// users of the public API keep working without changes.
pub use synthia_tool_bash::*;
pub use synthia_tool_exec_base::*;
```

This makes the shim *additive*: any type that exists at either new crate's
top level is reachable through the old `synthia-exec` path. There is no
`#[deprecated]` annotation because the project memory rule explicitly
forbids backwards-compat shims that are then removed later; the shim
stays indefinitely.

## 3. File-level moves

| Source | Destination |
|--------|-------------|
| `crates/synthia-exec/src/bash_tool.rs` | `crates/synthia-tool-bash/src/bash_tool.rs` |
| `crates/synthia-exec/src/command_blacklist.rs` | `crates/synthia-tool-bash/src/command_blacklist.rs` |
| `crates/synthia-exec/src/command_manager.rs` | `crates/synthia-tool-bash/src/command_manager.rs` |
| `crates/synthia-exec/src/monitor.rs` | `crates/synthia-tool-bash/src/monitor.rs` |
| `crates/synthia-exec/src/exec/` | `crates/synthia-tool-exec-base/src/exec/` |
| `crates/synthia-exec/tests/bash_utf8_panic.rs` | `crates/synthia-tool-bash/tests/bash_utf8_panic.rs` |
| `crates/synthia-exec/Cargo.toml` | both new crates (split) |
| `crates/synthia-exec/src/lib.rs` | reduced to 6-line shim |

## 4. Cargo.toml changes

### 4.1 New: `crates/synthia-tool-bash/Cargo.toml`

```toml
[package]
name = "synthia-tool-bash"
version.workspace = true
edition.workspace = true

[dependencies]
synthia-core = { path = "../synthia-core" }
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true
chrono.workspace = true
uuid = { version = "1", features = ["v4"] }
parking_lot.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

### 4.2 New: `crates/synthia-tool-exec-base/Cargo.toml`

```toml
[package]
name = "synthia-tool-exec-base"
version.workspace = true
edition.workspace = true

[dependencies]
synthia-core = { path = "../synthia-core" }
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true
uuid = { version = "1", features = ["v4"] }
priority-queue = "2"
parking_lot.workspace = true
toml = "0.8"
jsonschema = { version = "0.28", optional = true }

[target.'cfg(target_os = "linux")'.dependencies]
libc = "0.2"

[dev-dependencies]
tempfile.workspace = true

[features]
jsonschema-validation = ["dep:jsonschema"]
```

### 4.3 Reduced: `crates/synthia-exec/Cargo.toml`

```toml
[package]
name = "synthia-exec"
version.workspace = true
edition.workspace = true

[dependencies]
synthia-tool-bash = { path = "../synthia-tool-bash" }
synthia-tool-exec-base = { path = "../synthia-tool-exec-base" }
```

### 4.4 Workspace: `Cargo.toml`

Replace `"crates/synthia-exec"` with two entries (preserve order):

```toml
    "crates/synthia-tool-bash",
    "crates/synthia-tool-exec-base",
```

## 5. Source-level adjustments

### 5.1 `synthia-tool-bash/src/lib.rs`

```rust
pub mod bash_tool;
pub mod command_blacklist;
pub mod command_manager;
pub mod monitor;

pub use bash_tool::BashTool;
pub use command_blacklist::CommandBlacklist;
pub use command_manager::CommandManager;
pub use monitor::MonitorTool;
```

(verbatim from old `synthia-exec/src/lib.rs` minus the `validate_parameters` re-export)

### 5.2 `synthia-tool-exec-base/src/lib.rs`

```rust
pub mod exec;

pub use exec::validate_parameters;
```

(verbatim from old `synthia-exec/src/lib.rs` minus the tool re-exports)

### 5.3 Test file path update

`tests/bash_utf8_panic.rs`:
```diff
-use synthia_exec::{bash_tool::BashTool, command_blacklist::CommandBlacklist};
+use synthia_tool_bash::{bash_tool::BashTool, command_blacklist::CommandBlacklist};
```

### 5.4 Internal `use crate::*` paths in moved files

Files moved between crates must keep their internal `use crate::...`
references intact because they remain siblings inside the same crate.
Verified by reading the files:

- `bash_tool.rs`: `use crate::{command_blacklist::CommandBlacklist, command_manager::CommandManager};`
  → stays as-is.
- `monitor.rs`: `use crate::command_manager::CommandManager;`
  → stays as-is.
- `command_blacklist.rs`: no `use crate::...` references.
- `command_manager.rs`: no `use crate::...` references.
- `exec/executor.rs`: `use crate::exec::{...}` (line 381) → stays as-is
  (still inside `synthia-tool-exec-base`).

## 6. Verification matrix

| Check | Command | Expected |
|-------|---------|----------|
| All crates compile | `cargo check --workspace` | 0 errors |
| New crate tests | `cargo test -p synthia-tool-bash` | 0 regressions; `bash_utf8_panic` still passes |
| New crate tests | `cargo test -p synthia-tool-exec-base` | All existing `exec::` tests pass |
| Shim still works | `cargo test -p synthia-exec` | 0 regressions; `synthia_exec::BashTool` still resolvable |
| Workspace tests | `cargo test -p synthia-context -p synthia-agent -p synthia-tool` | 0 regressions |
| Lint | `cargo clippy --all-targets --all-features --tests -p synthia-tool-bash -p synthia-tool-exec-base -p synthia-exec` | 0 new warnings |
| Format | `cargo +nightly fmt --all -- --check` | 0 diff |
| OpenSpec | `openspec validate split-synthia-exec-crates` | green |

## 7. Risk register

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| Path import breaks in moved files | Low | Section 5.4 enumerates the 5 internal `use crate::*` sites; all are intra-crate so they survive the move. |
| `Cargo.lock` churn triggers surprising dep rebuilds | Low | Only adds 2 thin crates; no new external dependencies. |
| `synthia-exec` shim causes circular import | None | Shim only `pub use`s; no `use synthia_exec::...` anywhere in the workspace. |
| Test file in `tests/` dir can't find the moved lib | Low | Each new crate's `tests/` resolves against its own lib (Rust crate boundary). Verified path: `use synthia_tool_bash::...`. |
