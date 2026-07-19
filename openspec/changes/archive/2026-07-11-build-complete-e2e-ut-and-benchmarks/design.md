## Context

Synthia has 22 existing test files in `crates/synthia-agent/tests/` covering core agent behaviors (e2e_llm_test, react_loop_test, e2e_event_sequence_test, etc.) and unit tests scattered through each crate's `tests.rs` or inline `#[cfg(test)]` modules. However:

1. **No unified benchmark harness** — performance regressions in agent loop latency, token throughput, or session creation speed go undetected until production.
2. **Inconsistent test organization** — e2e vs unit test distinction is ad-hoc; no clear naming convention or categorization rule.
3. **Coverage gaps** — several modules (e.g., `synthia-tool`, `synthia-memory`, `synthia-context`) lack dedicated test files despite having non-trivial logic.
4. **No regression gates** — benchmarks run manually; no CI-integrated performance regression detection.

This change adds the missing test infrastructure without modifying any production code behavior.

## Goals / Non-Goals

**Goals:**

- Establish a `benches/` directory in `synthia-agent` using `criterion` for statistical rigor.
- Define clear test categorization: unit tests (`<module>_test.rs` in `src/`), integration tests (`<feature>_integration_test.rs`), e2e tests (`e2e_<scenario>_test.rs` in `tests/`).
- Add e2e tests covering missing scenarios: session pause/resume, concurrent tool calls, guardian permission gate, memory compaction trigger, and session teardown.
- Add unit tests for uncovered modules in `synthia-tool`, `synthia-memory`, and `synthia-context`.
- Produce a `tests/README.md` at workspace root documenting coverage categories and run commands.
- Register benchmarks in CI (or document how to run locally) with regression gates.

**Non-Goals:**

- Modifying production code to make it more testable (no behavioral changes).
- Adding benchmarks for every function (focus on agent loop and hot paths).
- Achieving 100% coverage (this is a structured improvement, not a coverage crusade).
- Replacing existing test infrastructure (cargo test still works identically).

## Decisions

### D1: Benchmark framework

- **Choice**: Use `criterion` for Rust benchmarks, with `iai` as a future option for memory/cycles profiling.
- **Reason**: `criterion` is the Rust standard for statistically rigorous latency/throughput benchmarks; it generates charts and detects regressions.
- **Alternatives considered**:
  - `test` benchmark harness: rejected because it lacks statistical analysis and chart output.
  - `criterion` + `rustmark` combo: deferred; `criterion` alone suffices for Phase 1.

### D2: Test file naming convention

- **Choice**: Use `e2e_<scenario>_test.rs` for end-to-end tests in `tests/`, `<module>_test.rs` for unit tests in the same dir as the code, and `<feature>_integration_test.rs` for integration-level tests that span multiple modules.
- **Reason**: Matches existing Synthia conventions (e.g., `e2e_llm_test.rs`, `e2e_event_sequence_test.rs`) and is self-documenting.
- **Alternatives considered**:
  - Flat `tests/` directory with all types mixed: rejected; categorization by filename prefix is clearer.

### D3: Where to put benchmarks

- **Choice**: `crates/synthia-agent/benches/` with a `benches/lib.rs` that re-exports all benchmarks, and individual `benches/loop.rs`, `benches/session.rs`, etc.
- **Reason**: Keeps benchmarks co-located with the crate they measure; avoids polluting `src/`.
- **Alternatives considered**:
  - Workspace-level `benches/`: rejected because benchmarks should be crate-specific for clear ownership.
  - `synthia-bench` binary: deferred; a `[[bench]]` target in `Cargo.toml` is simpler for Phase 1.

### D4: CI integration for benchmarks

- **Choice**: Document the benchmark run command and recommend running benchmarks on PRs that touch agent/session/tool code. Phase 1 does not block merges on benchmark regressions (informational only).
- **Reason**: Statistical noise in CI environments makes benchmark gating fragile; manual review is safer until the harness stabilizes.
- **Alternatives considered**:
  - Hard CI gate: deferred; requires baseline stabilization over multiple runs first.

## Risks / Trade-offs

- **[Risk] Benchmark noise in CI** → Mitigation: mark Phase 1 benchmarks as informational; document that local runs are authoritative.
- **[Risk] Test maintenance burden increases** → Mitigation: keep naming conventions simple; automated CI checks formatting only.
- **[Trade-off] More test files** → Accepted; better coverage outweighs disk cost.
- **[Trade-off] No hard coverage threshold** → Accepted; coverage metrics can be gamed; meaningful tests matter more than percentage.

## Migration Plan

This change is purely additive; no migration or rollback steps required.

1. Add benchmark harness with a warm-up baseline run.
2. Add e2e tests one scenario at a time, verifying each with `cargo test`.
3. Add unit tests for previously uncovered modules.
4. Document everything in `tests/README.md`.
5. Rollback: `git revert`; benchmarks and tests are not relied on by production code.

## Open Questions

- Should we use `proptest` for property-based tests on core data structures (TurnId, SessionId)?
- Do we want `cargo-fuzz` integration for adversarial input testing?
- Should benchmarks be runnable as part of `cargo test --workspace` or kept separate?
