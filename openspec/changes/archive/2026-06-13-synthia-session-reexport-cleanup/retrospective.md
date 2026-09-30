# Retrospective: synthia-session-reexport-cleanup

Date: 2026-06-13

## What went well

### The three-layer defense pattern is reproducible

This change established a reusable pattern for guarding structural
invariants that a single `cargo test` cannot fully cover. The
three layers each catch a different class of failure:

- **Layer 1 (doc tests)**: catches compile-time errors of the
  FORBIDDEN patterns. Cheap to add, runs on `cargo test --doc`.
- **Layer 2 (integration test)**: catches runtime / type-level
  errors and CANONICAL path drift. Catches what doc tests miss
  (e.g. someone renaming `types::Session` breaks Layer 2 loudly
  even if Layer 1 is silently bypassed).
- **Layer 3 (CI script)**: catches structural drift in the source
  file itself (e.g. someone re-adding the offending `pub use`
  line, or some other consumer introducing an unqualified
  import). Runs in < 100ms, no Rust toolchain needed.

This pattern is recommended for future structural invariants
(API surface guarantees, module boundary rules, naming
conventions enforced at the workspace level).

### Dead code deletion was uncontroversial

The two dead-code modules (`memory_pipeline` and `scheduler`) had
been orphaned since the `Session` shadowing was introduced. Their
deletion was straightforward: 2745 lines of code that no one
referenced and that no one would ever wire up (because doing so
would re-trigger the shadowing bug). This is a strong signal that
the original code review missed a structural change, but more
importantly, the deletion now is a 1-line `git revert` away from
recovery, so the cost of being wrong is low.

### Doc tests are an underused defense

The `compile_fail` doc test feature is built into Rust, runs
automatically, requires no external dependencies, and is impossible
to bypass without deliberate effort (you have to delete the doc
test). For "this pattern must NEVER compile" invariants, doc tests
are strictly better than a custom clippy lint.

## What was harder than expected

### Naming the policy in a way future contributors will read

The first iteration of the policy block was a 10-line comment
listing the 3 conflict names. The final version is a 30-line
comment with: a header, a table of the 3 conflict types, a list
of forbidden patterns, a checklist for adding new re-exports,
and a "policy enforcement" section explaining the 3 layers.

The bigger version is better because it answers the "why" before
the "what", which is the order future contributors will read it
in. The 10-line version felt like a "rule" that invited pushback
("why is this rule?"). The 30-line version is a "rationale" that
invites understanding.

### Getting the doc test snippet syntax right

Rust doc tests have surprising syntax requirements:

- `compile_fail` requires the snippet to NOT compile when
  combined with the surrounding module. The `fn _name() {}`
  pattern is needed to make the snippet a complete item.
- `///` doc comments inside the snippet must be on lines that
  start with `///` (not just `//`); otherwise the snippet is
  interpreted as ending the doc test.
- The `use ... as _Alias;` trick (alias to `_`) prevents the
  "unused import" warning from polluting the doc test output.
- The `compile_fail` annotation only fails the test if the
  snippet fails to compile. If the snippet compiles, the test
  fails with a message like "this `compile_fail` test
  successfully compiled". The historical offender doc test
  uses this to assert that the `pub use session::{...SessionManager...}`
  line causes the import to resolve (which would be a policy
  violation).

### Deciding what to put in `lib.rs` vs. tests

The policy block lives in `lib.rs` because it documents the
source. The structural assertion (`test_lib_rs_documents_policy`)
lives in the integration test because it asserts the documentation
is correct. This separation is clean: `lib.rs` is the "what", the
integration test is the "is the what still true?".

## Lessons learned

### 1. Multi-ownership types need qualified-path discipline

This is the core lesson. When a crate has multiple modules that
each define a type with the same name (in our case `Session`,
`SessionManager`, `SessionError` across `types`, `session`, and
`manager`), the crate root MUST NOT re-export these by name. Either:

- Don't re-export at all (consumers use qualified paths)
- Re-export from one and only one module (single ownership)

The first option is more flexible because it allows each module
to evolve independently. The second option is more convenient for
consumers but creates a coupling between the re-export and the
underlying module.

The current code chose option 1 for `SessionManager` and
`SessionError` (qualified paths only) and option 2 for `Session`
(single ownership: `types::*` glob). This is a reasonable
trade-off because `Session` is the most commonly used name and
qualifying it would be noisy, while `SessionManager` and
`SessionError` are more specialized and their short paths were
not relied upon.

### 2. Doc tests are a first-class defense for "must NEVER compile" invariants

Custom clippy lints are powerful but require nightly, a separate
crate, and a compiler plugin. Doc tests require nothing and run
on stable. For simple pattern-detection invariants, doc tests are
strictly better.

### 3. Dead code with broken imports is worse than no code at all

The two dead-code modules (`memory_pipeline` and `scheduler`) were
referencing `synthia_session::SessionManager` (the trait) with
code patterns that assumed a struct API. Had anyone wired them up,
they would have hit the shadowing bug at compile time and either
fixed the imports (best case) or given up (worst case).

Carrying dead code is sometimes necessary (e.g. when an in-flight
PR depends on it), but in this case the code had been dead for
months and no one was working on it. Deleting now was the right
call.

### 4. Self-enforcing policies are worth the upfront cost

The 3-layer guard added ~310 lines of code (6 doc tests + 1
integration test file + 1 shell script). In exchange, we get:

- Compile-time enforcement (doc tests)
- Test-time enforcement (integration test)
- CI-time enforcement (shell script)
- Documentation in the source (`lib.rs` policy block)

For a policy that protects against a class of bug that took
hours to diagnose and that was easy to re-introduce, this is
clearly worth the cost. The same approach should be considered
for other structural invariants (e.g. "no panic in non-test code",
"all public API functions have a doc comment", "no `unwrap` in
non-test code").

## What we would do differently next time

### 1. Use the three-layer pattern from the start

For future "fix the re-export / module boundary / naming
convention" type changes, the three-layer pattern should be the
default. It is cheaper to add the layers proactively than to
discover the bug a second time and have to retro-fit them.

### 2. Make the policy block a separate file

The 30-line policy block in `lib.rs` is starting to push against
the "first 20 lines of a Rust file should be `use` statements"
convention. A future improvement is to extract the policy to
`crates/synthia-session/POLICY.md` and link to it from `lib.rs`
with a one-line summary.

### 3. Consider a workspace-level clippy lint for the most common violations

The most common violation we worry about is "someone adds a
`pub use module::Foo` for a type that exists in another module".
A workspace-level clippy lint could catch this at compile time
without requiring the three layers. But this requires nightly
and a separate crate, so the cost-benefit is unclear for a
single invariant.

## Follow-up items (out of scope)

- The 4 pre-existing test failures on master
  (`test_multi_turn_memory_with_tracking_provider`,
  `test_react_loop_emits_llm_deltas`,
  `test_react_loop_respects_max_iterations`,
  9 doc tests in `synthia_agent::error::AgentError::*`) are
  tracked in `project_memory.md` and remain unfixed.
- The `synthia-session/tests/session_persistence.rs` and
  `synthia-session/tests/session_manager_integration.rs` files
  already use qualified paths (`types::Session`,
  `manager::SessionManager`) for the reasons outlined in
  `project_memory.md` ("synthia-session has pre-existing dual
  Session AND dual SessionManager re-export shadowing"). No
  further changes are needed in those files.
- The `openspec/` directory is gitignored. The artifacts
  created by this change are subject to the same worktree-loss
  risk documented in `project_memory.md` ("OpenSpec `openspec/`
  is gitignored"). Consider committing the artifacts through the
  active change directory or regenerating them in the main repo
  before worktree cleanup.
