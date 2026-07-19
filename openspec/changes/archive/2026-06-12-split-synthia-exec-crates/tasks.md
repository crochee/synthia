# Tasks: split `synthia-exec` into `synthia-tool-bash` + `synthia-tool-exec-base`

## 1. Create `synthia-tool-bash` crate

- [x] 1.1 Create `crates/synthia-tool-bash/Cargo.toml` (deps: synthia-core, tokio, serde, serde_json, thiserror, tracing, chrono, uuid, parking_lot; dev: tempfile)
- [x] 1.2 Create `crates/synthia-tool-bash/src/lib.rs` re-exporting `BashTool`, `CommandBlacklist`, `CommandManager`, `MonitorTool`
- [x] 1.3 Move `crates/synthia-exec/src/bash_tool.rs` → `crates/synthia-tool-bash/src/bash_tool.rs`
- [x] 1.4 Move `crates/synthia-exec/src/command_blacklist.rs` → `crates/synthia-tool-bash/src/command_blacklist.rs`
- [x] 1.5 Move `crates/synthia-exec/src/command_manager.rs` → `crates/synthia-tool-bash/src/command_manager.rs`
- [x] 1.6 Move `crates/synthia-exec/src/monitor.rs` → `crates/synthia-tool-bash/src/monitor.rs`

## 2. Create `synthia-tool-exec-base` crate

- [x] 2.1 Create `crates/synthia-tool-exec-base/Cargo.toml` (deps: synthia-core, tokio, serde, serde_json, thiserror, tracing, chrono, uuid, priority-queue, parking_lot, toml; optional: jsonschema; target-linux: libc; dev: tempfile)
- [x] 2.2 Create `crates/synthia-tool-exec-base/src/lib.rs` re-exporting `validate_parameters` and `pub mod exec`
- [x] 2.3 Move `crates/synthia-exec/src/exec/` → `crates/synthia-tool-exec-base/src/exec/` (5 files: mod.rs, executor.rs, executor_types.rs, priority.rs, validation.rs)

## 3. Reduce `synthia-exec` to a shim

- [x] 3.1 Replace `crates/synthia-exec/src/lib.rs` with 6-line shim (`pub use synthia_tool_bash::*; pub use synthia_tool_exec_base::*;`)
- [x] 3.2 Replace `crates/synthia-exec/Cargo.toml` with minimal version (deps: synthia-tool-bash, synthia-tool-exec-base; no src deps; no jsonschema, libc, toml, etc.)

## 4. Move integration test

- [x] 4.1 Move `crates/synthia-exec/tests/bash_utf8_panic.rs` → `crates/synthia-tool-bash/tests/bash_utf8_panic.rs`
- [x] 4.2 Update import: `synthia_exec::*` → `synthia_tool_bash::*`

## 5. Workspace

- [x] 5.1 Replace `"crates/synthia-exec"` entry in `Cargo.toml` `[workspace]` with `"crates/synthia-tool-bash"` and `"crates/synthia-tool-exec-base"`
- [x] 5.2 Verify `Cargo.lock` regenerates cleanly (compile passes)

## 6. Verification

- [x] 6.1 `cargo check --workspace` → 0 errors
- [x] 6.2 `cargo test -p synthia-tool-bash` → 0 regressions; `bash_utf8_panic` integration test (5 cases) passes
- [x] 6.3 `cargo test -p synthia-tool-exec-base` → all `exec::validation` and `exec::executor` unit tests pass
- [x] 6.4 `cargo test -p synthia-exec` → shim works; passes (32 lib tests inherited from the moved module structure)
- [x] 6.5 `cargo test -p synthia-context -p synthia-agent --lib` → 0 regressions (518 + 491 = 1009 tests)
- [x] 6.6 `cargo clippy --all-targets --all-features --tests -p synthia-tool-bash -p synthia-tool-exec-base -p synthia-exec` → 0 new warnings
- [x] 6.7 `cargo +nightly fmt --all -- --check` → 0 diff
- [x] 6.8 `openspec validate split-synthia-exec-crates` → green

## 7. Commit + archive

- [ ] 7.1 Single commit "refactor(workspace): split synthia-exec into synthia-tool-bash + synthia-tool-exec-base"
- [ ] 7.2 `openspec archive split-synthia-exec-crates --yes` and verify spec syncs
- [ ] 7.3 Write `retrospective.md` with metrics + lessons
- [ ] 7.4 Commit retrospective (if not in archive)
