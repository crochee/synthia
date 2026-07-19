<!--
Raw capture of brainstorming for "synthia-session-reexport-cleanup".

This file captures the exploration and conclusions in a decision log
format (background → decision chain Q1-Qn → design trade-offs → open
questions). Downstream artifacts (proposal.md, design.md, tasks.md)
extract and restructure the content.
-->

# Brainstorm: synthia-session re-export cleanup

> Date: 2026-06-13
> Author: Assistant (driven by user task: "清理 synthia_session::lib.rs
> 的 re-export 设计本身（避免未来类似陷阱）")
> Schema: superpowers-bridge

---

## 0. Background

`project_memory.md` documents a known follow-up:
"synthia-session has pre-existing dual Session AND dual SessionManager
re-export shadowing: explicit `pub use session::{Session, SessionError,
SessionManager}` in lib.rs shadows `pub use manager::*` glob, so
`synthia_session::SessionManager` resolves to the trait instead of
the struct. This caused 40+ compile errors in
`tests/session_persistence.rs` and `tests/session_manager_integration.rs`."

The previous change (`explicit-recovery-paths`, archived
2026-06-13) fixed the test compilation by using qualified paths in the
test files (`synthia_session::types::Session`,
`synthia_session::manager::SessionManager`). But the structural bug
in `lib.rs` was left in place — meaning the next contributor to add
a `pub use` line could re-trigger the same shadowing.

The user task: "清理 synthia_session::lib.rs 的 re-export 设计本身
（避免未来类似陷阱）" — clean up the re-export design itself AND
prevent future occurrences.

## 1. Decision chain

### Q1: How invasive should the fix be?

Options:
- A. Surgical: delete the offending line + update 2-3 consumers
- B. Comprehensive: delete the offending line + add 3-layer guard
  + delete dead code that referenced the shadowed types
- C. Architectural redesign: split `synthia-session` into separate
  crates for `types` / `manager` / `session` so name shadowing is
  impossible at the type-system level

Decision: **B**.

Reasoning:
- A is too small. It fixes the immediate bug but doesn't prevent
  the next one. The user explicitly said "避免未来类似陷阱"
  (avoid similar traps in the future), which requires a guard.
- C is too big. The user said earlier in the project (per
  `project_memory.md` "Workflows: First fix critical bugs and
  remove duplicate code, then discuss architectural abstractions
  after a stabilization period (6 months)") that architectural
  abstractions should be deferred 6 months. We are not at the 6
  month mark for the `synthia-session` crate, so the crate-split
  refactor is premature.
- B is the right size: fixes the bug, adds a guard, and removes
  dead code that would otherwise re-trigger the bug.

### Q2: What form should the guard take?

Options:
- A. Custom clippy lint
- B. compile_fail doc tests + integration test + CI script (3 layers)
- C. Single CI script (e.g. `grep` + `awk`)
- D. Single integration test (e.g. `trybuild` compile-fail test)

Decision: **B**.

Reasoning:
- A is the most powerful but requires nightly Rust, a separate
  `clippy_lints` crate, and a compiler plugin. Cost-benefit is
  poor for a single structural invariant.
- C is cheap but catches only the structural pattern (the
  offending `pub use` line). It doesn't catch the type-level
  consequences (someone renaming `types::Session` to
  `types::SessionModel` would not be caught by a CI script that
  just greps for the offending line).
- D is close to B but lacks the structural check. `trybuild` is
  a great tool but adds a dependency and is overkill for "this
  pattern must NEVER compile" (a 3-line doc test achieves the
  same with no dependencies).
- B is a "belt and suspenders" approach. Each layer catches a
  different class of failure:
  - Layer 1 catches the FORBIDDEN patterns at compile time
  - Layer 2 catches the CANONICAL paths at test time
  - Layer 3 catches structural drift at CI time

### Q3: Should we delete the dead code?

Options:
- A. Yes, delete both `memory_pipeline` and `scheduler`
- B. No, leave it for a future PR

Decision: **A**.

Reasoning:
- Both modules have been dead since at least 2026-06-13 (they
  were never declared in their respective `lib.rs`).
- Both reference `synthia_session::SessionManager` (the trait)
  in ways that wouldn't compile if wired up.
- Carrying 2745 lines of broken-on-purpose code is a future trap.
- Deleting is a 1-line `git revert` away from recovery, so the
  cost of being wrong is low.

### Q4: What should the policy block in `lib.rs` look like?

Options:
- A. Short (10 lines): just list the 3 conflict names
- B. Long (30 lines): include rationale, canonical paths,
  rule, and link to the 3 layers

Decision: **B**.

Reasoning:
- Future contributors will read the policy block. A 10-line
  block is a "rule" that invites pushback ("why is this rule?").
  A 30-line block is a "rationale" that invites understanding.
- The longer block also serves as documentation for the
  three-layer guard (it links to all three layers and explains
  why each exists).

### Q5: Should the 3 layers be kept in sync manually, or should
there be a test that asserts they are in sync?

Options:
- A. Manual: trust the contributor to update all 3 layers
- B. Add a 4th test that asserts the 3 layers are consistent
  (e.g. layer 2 has the same forbidden names as layer 1)

Decision: **A** (for now).

Reasoning:
- The 3 layers are intentionally redundant, not strictly
  consistent. Layer 1 checks "forbidden patterns", Layer 2
  checks "canonical paths", Layer 3 checks "structural drift".
  The sets are related but not identical.
- A "consistency check" test would either be too loose (just
  check that all 3 layers mention the 3 names) or too tight
  (check that they have the same forbidden patterns, which
  would couple them tightly).
- The simpler approach is to add a NOTE comment in each layer
  pointing to the other two, and trust the contributor to
  update all three.

## 2. Design trade-offs

### Trade-off 1: doc tests vs. custom clippy lint

Custom clippy lint:
- Pro: catches the bug at compile time, before any test runs
- Pro: integrated with `cargo build` / `cargo check`
- Con: requires nightly Rust
- Con: requires a separate `clippy_lints` crate
- Con: requires a compiler plugin

`compile_fail` doc tests:
- Pro: zero dependencies, runs on stable
- Pro: impossible to bypass without deleting the doc test
- Con: only catches "this pattern must NEVER compile"
- Con: doesn't catch the structural pattern (e.g. someone adding
  the `pub use` line back)

Conclusion: doc tests are better for the FORBIDDEN patterns.
The structural pattern is caught by the CI script (Layer 3).

### Trade-off 2: integration test vs. trybuild

Integration test:
- Pro: zero dependencies
- Pro: catches drift in canonical paths and policy block content
- Con: doesn't catch "this pattern must NEVER compile" (you can
  add the offending line and the test still passes, because the
  test doesn't try to compile the forbidden pattern)

`trybuild`:
- Pro: catches both "must compile" and "must NOT compile"
- Con: adds a dependency
- Con: complex test setup

Conclusion: integration test is enough for our purposes because
the FORBIDDEN pattern check is already covered by Layer 1
(doc tests). Layer 2 focuses on CANONICAL path drift and
structural drift (via `include_str!` + grep), which a plain
integration test can handle.

### Trade-off 3: how to verify the 3 layers actually catch the bug

Two mutation tests were run during development:
- Mutation 1: re-introduce the offending `pub use` line — Layer 2
  test failed to compile
- Mutation 2: add a violating unqualified import in a fake file
  — Layer 3 script exited 1

Both mutations were reverted after verification. This is the
"negative testing" pattern: prove that the guard catches the bug
before relying on it.

## 3. Open questions

None. The change is small, surgical, and self-contained. The 3
layers are designed to be maintainable by future contributors
without further architectural changes.

## 4. Follow-up items (out of scope)

- 4 pre-existing test failures on master (see `verify.md`)
- `openspec/` is gitignored (artifacts may be lost on worktree
  cleanup, see `project_memory.md` lesson learned)
- `synthia-session` may eventually be split into separate crates
  to make name shadowing impossible at the type-system level.
  This is a 6-month-deferred architectural refactor per the
  project memory.
