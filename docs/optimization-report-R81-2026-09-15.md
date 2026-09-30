# Optimization report R81 — 2026-09-15

## What the audit found

R76–R80 closed five drift surfaces (judge parser, char
truncator, partial redactor, cursor codec, MAX_LIMIT
constant) by collapsing them into `synthia-core`. The
R73/R74 rounds added new functionality (LLM judge scorer
with 8 tests, R74 argument validation with 5 tests); both
rounds dropped the new tests into the existing
`synthia_tool::registry::tests` block. That block was
already 1692 lines / 56 822 bytes — the largest `mod tests`
in the workspace.

The R74 block (198 lines, lines 2862–3059) sat at the
*end* of the test block, scoped under a `// ── Pre-dispatch
JSON-Schema validation (R74) ───` section comment. It
already had a focused home in the file (the section
header), but it was structurally identical to the
surrounding tests — a `#[test] fn ...` inside one big
`mod tests` block. A new contributor looking for the
"argument validation" tests had to scroll to the end of
the file.

The right move is to **promote the R74 block to a sub-
module**, with the section header becoming the sub-module
name. The R74 tests become a `mod argument_validation`
inside the parent `mod tests` block:

```text
tests::argument_validation::dispatch_passes_arguments_…
tests::argument_validation::dispatch_synthesises_…
tests::argument_validation::dispatch_runs_the_tool_…
tests::argument_validation::clone_preserves_…
tests::argument_validation::validation_passes_…
```

`cargo test argument_validation` now jumps straight to
the R74 tests without scrolling. The parent `mod tests`
block shrinks by ~200 lines and gains a one-line
marker pointing at the new sub-module.

This is the **first** of a planned series of test-block
splits (R82+ will move the dispatch tests, the snapshot
tests, etc. into their own sub-modules). The R74 split
is the smallest, most self-contained section — the tests
are recent, the section header was already there, and the
fixture (`ShellTool`) lives entirely within the block.
Future rounds can build on the same pattern.

## What landed

- New sub-module `mod argument_validation` inside
  `synthia_tool::registry::tests`. Contains:
  - The `ShellTool` fixture (a tool with a typed
    schema: `required: ["cmd"]`, `cmd: string`,
    `timeout: integer`).
  - The 5 R74 tests:
    `dispatch_passes_arguments_to_the_tool_when_validation_is_off`,
    `dispatch_synthesises_schema_violation_error_when_validation_is_on`,
    `dispatch_runs_the_tool_when_validation_passes`,
    `clone_preserves_argument_validation_flag`,
    `validation_passes_through_empty_schemas`.
- `use super::*;` brings the parent block's `Tool`,
  `ToolOutput`, `Context`, `ToolRegistry`, `ToolEntry`,
  `TestEntryTool`, `collect_results`, etc. into the
  sub-module. `use async_trait;` is repeated because the
  parent block uses it for the trait derivation (and
  `use super::*;` does not re-export `async_trait`).
- One-line marker in the parent block at the original
  position: `// ── R74 argument-validation tests moved to
  \`mod argument_validation\` above.`

## Why this is a low-risk refactor

- The R74 tests are **isolated** from the rest of the
  test block: they touch only the public
  `with_argument_validation` / `argument_validation_enabled`
  / `run_stream` API and use no parent-scope state.
- The sub-module is `#[cfg(test)]`-only; it does not
  appear in the public surface or the test binary.
- The split does not change any test logic, any assertion,
  or any test name. The 5 tests still report as
  `argument_validation::*` in the test output (cargo
  groups sub-module tests under their module path).
- `make ci` runs the same gate suite. `make test-unit`
  reports 2447/2447 (unchanged — the split is purely
  organisational).

## Verification

- `cargo test -p synthia-tool --lib` 283/283 pass
  (unchanged total; the 5 R74 tests now report under
  `argument_validation::*`).
- `cargo run -q -p synthia-tool --example
  argument_validation` still prints
  `ARGUMENT-VALIDATION: OK`.
- `make ci` 6/6 green.
- `make test-unit` 2447/2447 (unchanged).
- `make check-public-api-runtime` clean — the
  sub-module is `#[cfg(test)]`, so it does not appear
  in the public surface.

## Deferred

- The rest of the `mod tests` block in
  `synthia_tool::registry` (≈ 1450 lines, 23 tests) is
  not split. It can be split into:
  - `mod exposure` (2 tests: lines 1406–1452)
  - `mod scope` (4 tests: lines 1551–1589)
  - `mod registration` (3 tests: lines 1651–1703)
  - `mod descriptor_cache` (2 tests: lines 1725–1799)
  - `mod snapshot_cache` (2 tests: lines 1800–1889)
  - `mod snapshot` (5 tests: lines 1890–1999)
  - `mod dispatch` (4 tests + ~10 streaming/edge tests:
    lines 2300–2700)
  - `mod mutations` (2 tests: lines 2769–2860)
  - `mod materialization` (1 test: line 2436)
  - `mod fixtures` (shared `TestEntryTool`, `ShadowTool`,
    `NamedTool`, the `Mock*` types)
  Each is a future round; the R74 split establishes
  the pattern (extracted block lives in a sub-module,
  parent block gains a one-line marker, no test
  logic changes, no new fixtures, no new public
  surface).
