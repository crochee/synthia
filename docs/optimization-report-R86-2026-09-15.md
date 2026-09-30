# Optimization report R86 — 2026-09-15

## What the audit found

R85 named `mod scope` / `mod descriptor_cache` /
`mod snapshot_cache` as the next R86 candidate. On
inspection, those three names mapped to line ranges in
the pre-R81 file that **already collapsed into the
sections R82–R85 extracted** (`mod snapshot`,
`mod registration`, `mod dispatch`, `mod mutations`).
The deferred list is now closed by the accumulated R82
+ R83 + R84 + R85 work — there is no remaining flat
section in the parent `mod tests` block.

This is the audit-closure round: no code moves, the
report records the closed state and names the next
candidate space.

## What the audit verified

The parent `mod tests` block at
`crates/synthia-tool/src/registry.rs` (lines 1369–3297)
now holds exactly:

- 8 sub-module declarations (`mod fixtures` + 7 others)
- 8 sub-module intro comments
- 8 one-line "moved to ... above" markers
- the parent-level `pub(crate) use fixtures::{...};`
  re-export

Zero `#[test]` / `#[tokio::test]` items live at the
parent block's indent (`^    #\[(test|tokio::test)\]`).
Every test now lives in one focused sub-module.

The 8 sub-modules, all verified by name-filter:

| Sub-module | Filter | Tests | Source round |
|---|---|---|---|
| `mod fixtures` | `cargo test -p synthia-tool --lib fixtures` | 0 (fixtures only — the 3 fixture impls are structs without tests) | R83 |
| `mod exposure` | `cargo test -p synthia-tool --lib exposure` | 7 passed | R82 |
| `mod session_scope` | `cargo test -p synthia-tool --lib session_scope` | 4 passed | R82 |
| `mod registration` | `cargo test -p synthia-tool --lib registration` | 24 passed | R84 |
| `mod snapshot` | `cargo test -p synthia-tool --lib snapshot` | 16 passed | R82 |
| `mod dispatch` | `cargo test -p synthia-tool --lib dispatch` | 17 passed | R85 |
| `mod mutations` | `cargo test -p synthia-tool --lib mutations` | 2 passed | R82 |
| `mod argument_validation` | `cargo test -p synthia-tool --lib argument_validation` | 5 passed | R81 |

73 tests filter by sub-module name (the remainder are
named differently — e.g., `test_tool_registry_register_and_get`
in `mod registration` doesn't match the substring).
`cargo test -p synthia-tool --lib` reports 283/283
after every commit in R81–R85.

## What landed

Nothing on the code side this round. The audit-closure
record (this file) plus the CHANGELOG `[Unreleased]`
entry and the `docs/README.md` round-map row are the
R86 deliverables.

The deferred list from R81 is closed:
`mod exposure` / `mod scope` / `mod registration` /
`mod descriptor_cache` / `mod snapshot_cache` /
`mod snapshot` / `mod dispatch` / `mod mutations` /
`mod materialization` / `mod fixtures` — every item
either landed as a sub-module (mod scope and mod
materialization were absorbed into other extractions
during the run; mod descriptor_cache and mod
snapshot_cache live inside `mod registration` and
`mod snapshot` respectively) or was rendered moot by
the R83 fixtures move (mod fixtures).

## What the audit looked at beyond the test block

The R86 audit did not stop at "deferred list closed".
It looked at the next-largest library-crate test files
to see if the same pattern has a follow-up there:

- `crates/synthia-agent/src/agent/re_act.rs` (2975
  lines). Declined in R64: "Readability > line count;
  the per-step naming (`prepare`, `sample_once`,
  `commit_assistant`, `execute_tools`, `finalize`) is
  the documentation strategy; splitting would put 2975
  lines across files that each still have to be read in
  sequence to understand." The same argument still
  holds.
- `crates/synthia-provider/src/openai_streaming/tests.rs`
  (943 lines, 22 tests, all flat). Each test has a
  focused doc-comment + setup + assert + regression
  rationale; reading top-to-bottom traces one component
  (`OpenAIStreamProcessor`) through its concerns. The
  same R64 reasoning applies: focused doc-comments on
  each test give the file its navigation. Splitting
  would scatter related coverage of the same component
  across multiple files for no real readability gain.
- `crates/synthia-tool/src/sandbox.rs` (1077 lines),
  `crates/synthia-tool/src/surface.rs` (951 lines).
  Library code, not tests. Not in scope for this
  family of rounds (R81–R85 were about test-block
  decomposition, not source-module decomposition).

No new candidate was found.

## What is still open

The standing objective is unaffected. This round did
not change parity, drift, or the architecture gates;
the test-block split was always an organisational tail
of the bigger project. Future rounds remain gated
on the same signals:

- Reference repos `traitclaw` / `pi` move → re-run the
  R29/R62 parity audit; any new capability lands via
  the same seambre the project already exposes.
- A new drift surface appears (the `MUST match` /
  `MUST stay in sync` grep, currently quiet outside
  test assertions) → land via the R76–R80 pattern
  (one canonical home in `synthia-core`, server / agent
  become thin delegations).
- A new architecture gate fails → land the fix; the
  gates (`make ci`, `make test-unit`, `make examples`,
  `make check-mvp-deps`, `make check-no-runtime`,
  `make check-public-api-runtime`, `make check-clock`)
  are the project's signal that the standing
  objective is being honoured.

Until one of those signals fires, no code work is
pending. The standing objective is **on track**, not
complete — completion is gated on the project's own
quality gates, which are all green.

## Verification

- `cargo test -p synthia-tool --lib`: 283 passed, 0
  failed (unchanged total).
- `make ci` 6/6 green (fmt-check, clippy `-D warnings`,
  doc-check, `--no-default-features` compile, MVP
  dependency invariant, runtime-free invariant,
  public-API runtime ban, clock ratchet at baseline 6).
- `make examples` exit 0; `CONSUMER-PROOF: OK` and
  `MVP-OK` proof lines printed.
- Reference repos `traitclaw` / `pi`: no commit since
  2026-09-10; R62/R64 parity audit still current.