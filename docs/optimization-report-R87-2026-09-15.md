# Optimization report R87 — 2026-09-15

## What the audit found

R86 closed the test-block split cycle and declared
the deferred list exhausted. With no further
organizational candidates, the audit looked at
AGENTS.md §3.2 directly:

> 新产生的 Rust 代码若未使用请直接删除；不得使用
> `dead_code` / `unused` 等属性抑制警告。

> New Rust code that is unused MUST be deleted; the
> `dead_code` / `unused` attributes must not be used
> to suppress warnings.

The audit grepped every `#[allow((dead_code|unused)]`
in library code and read each call site to verify
whether the attribute was hiding a real resource
holding (legitimate) or a phantom suppressor /
genuine dead code (violation).

- Reference parity remains closed (no `traitclaw` /
  `pi` commit since 2026-09-10).
- No drift surface (`MUST match` / `MUST stay in sync`)
  appeared.
- `make ci` was green at round start; the audit's only
  candidate was the standing AGENTS.md §3.2 contract.

## What landed

One commit (`e6851fd`) — six files, +12 / −74,
net **−62 lines** of code, **zero new warnings**.

### Three phantom dead-code suppressor functions removed

`#[allow(dead_code)] fn _foo() {}` markers that exist
only to silence a warning that doesn't exist:

- `crates/synthia-agent/src/agent/delegation.rs::_gate_module_link`
  — body `let _: Option<GateVerdict> = None;`. The
  comment claimed "items are reachable through
  `super::delegation` already", which is correct
  (they are) — the marker exists only to silence a
  warning that doesn't exist. Deleted.
- `crates/synthia-agent/src/agent/worktree.rs::_worktree_marker`
  — `pub(crate) fn _worktree_marker() {}` with comment
  "Helper for `cargo test`". Empty body, no callers,
  no use in tests. Deleted.
- `crates/synthia-context/src/summarizing.rs::_type_anchor`
  — empty fn whose parameters are typed `ToolUse`,
  `ToolResult`, `TextContent`. After deletion:
  `ToolUse` was genuinely unused in lib code (only
  doc-comment references remain) — removed from the
  import. `ToolResult` and `TextContent` ARE used in
  `mod tests` via `use super::*;`, so they moved from
  the parent `use` to a test-local `use synthia_provider::{...}`.

### Dead fields and variants pruned

`#[allow(dead_code)]` was hiding genuine dead code:

- `synthia-context::summarizing::ShortRef` — the
  attribute hid two dead fields (`tool_call_id`,
  `tool_name`), not a real unused import. Both fields
  were set in `allocate_short_refs` but only
  `short_id` was read in `build_summary_message`.
  `ShortRef` collapses to `{ short_id: String }`;
  `allocate_short_refs` loses its `messages`
  parameter and the per-message loop that populated
  the dead fields.
- `synthia-provider::assembler::PartialBlock` — the
  enum carried `Text` and `Reasoning` variants that
  were never constructed or matched (the assembler
  tracks `text` / `reasoning` as separate accumulator
  fields on `BlockAssembler`, not as `PartialBlock`
  entries). `PartialBlock` collapses to just `ToolUse`;
  the enum-level `#[allow(dead_code)]` is no longer
  needed.
- `synthia-scheduler::store::LockGuard` — the struct
  carried `root: PathBuf` and `job_id: JobId` fields
  that were set in `acquire_lock` but never read (the
  `Drop` impl only uses `self.path`). Both fields
  and their `#[allow(dead_code)]` attributes removed;
  the `LockGuard` initializer simplified.

### Dead helper deleted

- `synthia-skill::discovery::home_dir_fallback` —
  `fn home_dir_fallback() -> Option<PathBuf> { None }`
  with comment "reserved hook for systems that don't
  expose `HOME`". No callers anywhere in the workspace.
  Deleted.

### rustdoc fix

- `synthia-agent::agent::delegation.rs` line 84's
  intra-doc link `[GateVerdict]` broke after the
  `GateVerdict` import was removed; updated to point
  at the alias `[GateVerdict](DelegationGateVerdict)`
  that the `pub use` makes visible. The `make
  ci` `doc-check` gate (R62) caught this — exactly
  the kind of dead-link surface R87's discipline
  contract is supposed to surface.

## What was kept (legitimate suppressions)

Two suppressions survived the audit because they hold
real resources the `Drop` impl relies on:

- `synthia-mcp/src/stdio.rs::child: Mutex<Child>` —
  constructed (line 90), stored in struct, never
  read in Rust terms; purpose is the `Child::Drop`
  impl that kills the OS process on transport drop.
  Comment: "Never read directly. Held for `Drop`."
- `crates/synthia-context/src/summarizing.rs::allocate_short_refs`
  etc. — every other `#[allow(...)]` attribute in the
  grep hit `#[cfg(test)] mod tests` blocks, where
  `clippy::useless_conversion` / `clippy::collapsible_if`
  allowlists are standard test-discipline allowances.

The audit also verified:

- `synthia-provider/src/assembler.rs::PartialBlock`'s
  `Text` and `Reasoning` variants had zero matches
  anywhere in the workspace — the prune was safe.
- `LockGuard`'s `root` and `job_id` fields had zero
  readers — the prune was safe.
- `home_dir_fallback` had zero callers — the delete
  was safe.

## Why this is a low-risk refactor

- All six files compile with zero warnings. The
  `cargo build --workspace --all-targets` count went
  from a small handful of warnings down to **zero**.
- Test counts unchanged: 2447 tests passing.
- The `make ci` `doc-check` gate (R62) immediately
  caught the one rustdoc regression (broken
  intra-doc link in `delegation.rs`) and that was
  fixed in the same commit. Exactly the kind of
  cross-crate coupling the gates are supposed to
  surface.
- The dead-code audit is mechanical and re-runnable:
  every `#[allow((dead_code|unused))]` in library
  code outside test modules now has a documented
  purpose (resource ownership in a `Drop` impl) or
  doesn't exist.

## Verification

- `cargo build --workspace --all-targets`: zero
  warnings, zero errors.
- `cargo +nightly fmt --all --check`: clean.
- `make ci` 6/6 green (fmt-check, clippy `-D warnings`,
  doc-check, `--no-default-features` compile, MVP
  dependency invariant, runtime-free invariant,
  public-API runtime ban, clock ratchet at baseline 6).
- `make test-unit` exit 0 (2447 tests, unchanged).
- `make examples` exit 0; `CONSUMER-PROOF: OK` and
  `MVP-OK` proof lines printed.

## Deferred

Standing trigger conditions still hold:

- Reference repos `traitclaw` / `pi` move → re-run
  the R29/R62 parity audit.
- A new drift surface (`MUST match` / `MUST stay in
  sync`) appears → land via the R76–R80 pattern.
- An architecture gate fails → land the fix; the
  gates are the standing trigger signal.
- A new `#[allow((dead_code|unused))]` slips into
  library code → re-run the R87 grep. The audit is
  the one-line check
  `git grep -nE '#\[allow\((dead_code|unused)\)\]' crates/`
  outside `#[cfg(test)]` blocks.

R87 is the canonical pattern for closing future
violations of AGENTS.md §3.2 — every round a new
grep is the cheapest way to enforce it.