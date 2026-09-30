## 1. Clippy Cleanup

- [x] 1.1 Fix `unwrap_or_default` error at `synthia-agent/src/agent_tools.rs:290` — replace `or_insert_with(HashSet::new)` with `or_default()`
- [x] 1.2 Fix `bind_instead_of_map` error at `synthia-agent/src/agent_tools.rs:336` — replace `and_then(|x| Some(y))` with `map(|x| y)`
- [x] 1.3 Verify `cargo clippy --workspace -- -D warnings` passes

## 2. Registry Refactor

- [x] 2.1 Analyze `synthia-tool/src/registry.rs` (1193 lines) to identify natural module boundaries
- [x] 2.2 Create new module files: `tool_registry/` submodules for registration, validation, metadata
- [x] 2.3 Move code from `registry.rs` to new modules while preserving all public API
- [x] 2.4 Update `lib.rs` and `mod.rs` to re-export public types from new module locations
- [x] 2.5 Verify `cargo build -p synthia-tool` succeeds
- [x] 2.6 Run integration tests to confirm API compatibility

## 3. Architecture Audit

- [x] 3.1 Review `synthia-permission/src/lib.rs` — verify unified Permission enum structure and usage consistency
- [x] 3.2 Search codebase for any remaining `synthia-multiagent` references and remove/migrate them
- [x] 3.3 Audit `synthia-agent/src/task/scheduler.rs` vs `synthia-task` responsibilities — document boundary clarification
- [x] 3.4 Produce architecture audit summary with findings

## 4. Performance Analysis

- [x] 4.1 Measure baseline build time: `cargo clean && cargo build --workspace` — record elapsed time
- [x] 4.2 Analyze memory cold storage in `synthia-memory/src/cold.rs` — identify query patterns and optimization opportunities
- [x] 4.3 Evaluate embedding computation in `synthia-skill/src/embedding.rs` — identify batching/caching opportunities
- [x] 4.4 Produce performance optimization proposal with prioritized recommendations

## 5. Verification

- [x] 5.1 Run full test suite: `cargo test --workspace`
- [x] 5.2 Run clippy: `cargo clippy --workspace -- -D warnings`
- [x] 5.3 Confirm no regression in existing functionality