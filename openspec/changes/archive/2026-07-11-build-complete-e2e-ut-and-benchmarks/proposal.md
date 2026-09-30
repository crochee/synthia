---
slug: build-complete-e2e-ut-and-benchmarks
createdAt: "2026-07-11T00:00:00.000Z"
---

## Why

Synthia currently has a test surface covering the agent loop (react_loop_test, e2e_llm_test, e2e_event_sequence_test) but lacks unified benchmark infrastructure, systematic e2e test coverage for all interaction patterns, and granular unit tests for isolated modules. Production deployments need reproducible performance numbers and confidence that all agent behaviors are covered by automated tests. Adding these now closes the gap before the project reaches wider adoption.

## What Changes

**Comprehensive Test Coverage**
- From: spot-check unit tests and a handful of e2e tests with no unified benchmark harness.
- To: a structured test organization with clear coverage categories (unit/integration/e2e), automated benchmark runner, and regression gates.

**Unified Benchmark Infrastructure**
- From: no standard benchmark suite; performance regressions undetected until production.
- To: a `synthia-bench` binary (or bench module) using `criterion` or `iai` with clear measurement categories: agent loop latency, token throughput, session creation, event writing throughput.

**Test Organization**
- From: tests scattered ad-hoc across crate directories.
- To: a `tests/` hierarchy per crate with clear naming conventions (`<feature>_test.rs` for e2e, `<module>_test.rs` for unit tests) and a workspace-level `tests/README.md` documenting coverage scope.

## Capabilities

### New Capabilities

- `test-coverage-standards`: Defines naming conventions, categorization rules (unit/integration/e2e), and required coverage thresholds for each crate.
- `benchmark-harness`: Criterion-based benchmark suite with categorized benchmarks, statistical rigor, and regression detection.
- `e2e-test-suite`: Comprehensive e2e tests covering all major agent interaction patterns: single-turn, multi-turn, session resume, tool calls, error recovery, and session lifecycle.

### Modified Capabilities

- None. This change only adds test/benchmark infrastructure; no production behavior changes.

## Impact

- `crates/synthia-agent/tests/` gains structured e2e tests with consistent naming and categorization.
- `crates/synthia-agent/benches/` (new) hosts criterion benchmarks.
- `crates/synthia-core/`, `crates/synthia-session/`, `crates/synthia-tool/` gain targeted unit tests for uncovered modules.
- `tests/README.md` at workspace root documents coverage expectations and run commands.
- No public API or CLI behavior changes.
