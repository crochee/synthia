# R31 Absorption Results (+ the R32-opening repair record) — 2026-09-12

Plan: [`optimization-report-R31-2026-09-12-plan.md`](optimization-report-R31-2026-09-12-plan.md).
Commit: `75953f79` (`feat(R31): file-backed memory, sandbox policy, subagent pool, workflow runtime`).
Sequel: [`optimization-report-R32-2026-09-12.md`](optimization-report-R32-2026-09-12.md).

R31 shipped its four Wave-1 slices and was committed with **Wave 2
missing and the tree in a broken state**. This file records both:
what R31 actually delivered, and what the R32-opening audit found
and repaired before any new work started. Nothing below is inferred
— every claim is a command result quoted from that audit.

## What R31 delivered (Wave 1)

| Slice | Source | Landing |
|---|---|---|
| **M. File-backed scoped long-term memory** | pi-subagents §memory | `synthia-context::memory::file` — `FileMemory` (`user`/`project`/`local` scopes, bounded `MEMORY.md` index, deterministic keyword recall, ULID ids, read-only mode, symlink/traversal defenses) |
| **N. OS-level execution policy** | dsh §sandbox | `synthia-tool::sandbox` — `ExecutionPolicy`, `SandboxBackend::confine(policy, root) -> ConfinedCommand` (argv construction, not a wrapper process), `BwrapBackend`, fail-closed availability probing, `effective_policy` fold over session events |
| **O. Two-pool subagent concurrency** | pi-subagents §agent-manager | `synthia-agent::agent::pool` — `SubagentPool` admission state machine (bounded background lane + FIFO queue, unbounded foreground, uncharged nested children, 100 tombstones), `InvocationRecord` |
| **W. Declarative workflow runtime** | pi-subagents §workflow | new crate `synthia-workflow` — `WorkflowSpec` (agent / fan-out / pipeline), `WorkflowRuntime` over one `WorkflowHost` effect seam, caps + concurrency + `WorkflowControl`, JSONL `WorkflowJournal` with torn-tail tolerance and prefix replay |

**Not delivered:** the plan's Wave 2 — **P** (`GroupedRegistry`) and
**Q** (the progressive example ladder + `docs/examples/README.md`) —
plus the R31 results report, the README primitive rows, and the
CHANGELOG entry. Those are completed in R32 (see the R32 report for
P and Q; this file covers the repairs).

## Repair record (R32-opening audit, `75953f79` →)

The audit ran the gates R31 never ran. Four classes of defect, all
in the committed tree:

### 1. A corrupted test file

`crates/synthia-tool/tests/sandbox_smoke.rs` was **3096 NUL bytes**
(`git show 75953f79` reports it as `Bin 0 -> 3096 bytes`), so
`cargo +nightly fmt --all` failed to even parse the workspace
(`unknown start of token: \u{0}`). A repo-wide scan
(`git ls-files | NUL-byte grep`) found no other corrupted file in
the repository; two *cargo registry* extractions (`cc-1.4.3`, and by
extension the same class of partial extraction) were also affected
and were purged + re-downloaded.

The file was rewritten as a real smoke test: six tests driving
`ShellTool` through its **actual spawn path**, including a real
bubblewrap run (the host has `/usr/sbin/bwrap` and permits
unprivileged user namespaces), with a *capability* preflight — not
just `bwrap --version` — so a host that cannot create namespaces
skips loudly instead of pinning a spawn failure as a policy denial:

```text
test confined_policy_without_backend_fails_closed ... ok
test noop_backend_refuses_confined_policy ... ok
test full_access_runs_unconfined ... ok
test read_only_allows_reads ... ok
test read_only_denies_workspace_write ... ok
test workspace_write_scopes_the_grant ... ok
```

### 2. An unformatted tree

`cargo +nightly fmt --all --check` failed with ~20 hunks across
`synthia-agent/src/agent/pool.rs`, `synthia-workflow/src/{concurrency,journal,lib,plan,result,runtime}.rs`.
R31 was committed without running the formatter.

### 3. `synthia-workflow` did not compile

`cargo check -p synthia-workflow --all-targets` failed with 6 errors
— the crate had never been built:

- `concurrency.rs`: `head.waker.wake()` moved a `Waker` out of a
  shared reference (×2, in `release` and `leave_queue`) → now
  `wake_by_ref()`;
- `concurrency.rs` / `control.rs`: two tests declared `#[test] fn`
  while the module aliases `tokio::test`, so the async keyword was
  required → made `async fn`;
- `concurrency.rs` test: `async` block borrowed a `u32` parameter →
  the closure now reborrows the semaphore/trace and moves the tag;
- `concurrency.rs` test: `abandoned` was moved by `now_or_never()`
  and then `drop`ped → the redundant `drop` was removed (the future
  is dropped by `now_or_never` itself, which is the point of the
  test).

### 4. Broken behaviour and wrong expectations

- `synthia-agent/src/agent/pool.rs`: `tier_substituted()` matched on
  `self.requested_tier`/`self.effective_tier` **by value**, moving
  non-`Copy` `ModelTier`s out of `&self`; and `effective()` consumes.
  Fixed with a zero-copy `resolved(&ModelTier) -> &ModelTier`
  helper that walks the `Override` chain by reference — no clone, no
  allocation (an override clone would allocate).
- `synthia-agent/src/agent/delegation.rs`: four genuinely unused
  imports (`chrono::{DateTime, Utc}`, `TokenUsage`, `pool`,
  `SystemEvent`) — deleted.
- `synthia-agent::pool::release_drains_only_the_freed_lane` asserted
  `pool.is_idle()` after both lanes had handed their slot to a
  waiting child. The pool's own contract (`release` returns the
  ticket of the **newly admitted** child) makes that false: the
  corrected assertions pin the real invariant — `queued_len() == 0`,
  `counts().active() == 2`, `!is_idle()`.
- `synthia-workflow::the_limit_bounds_how_many_hold_at_once` was
  measuring a fully synchronous critical section, where every future
  completes inside one poll and the observed peak can never exceed
  one. The test now yields while holding the permit, so the bound is
  actually exercised (`peak == 2` for a limit of 2).

After the repairs: `cargo check --workspace --all-targets
--all-features` is clean (0 errors, 0 warnings) and
`cargo +nightly fmt --all --check` exits 0.

## Verification (post-repair, at the start of R32)

| Gate | Result |
|---|---|
| `cargo +nightly fmt --all --check` | exit 0 |
| `cargo check --workspace --all-targets --all-features` | 0 errors, 0 warnings |
| `cargo test -p synthia-tool` | 250 lib + 6 sandbox-smoke + integration — 0 failed |
| `cargo test -p synthia-agent` | 271 lib + integration — 0 failed |
| `cargo test -p synthia-workflow` | 19 — 0 failed |
| `cargo test -p synthia-context` | 93 — 0 failed |

## Lesson recorded

R31's four slices were substantial and sound in design; the failure
was procedural, not conceptual: the round was committed from a
working state that was never verified, one file arrived corrupted
from the filesystem, and three tests encoded expectations the code
never had. R32 therefore opens with the full gate sequence *before*
any new work, and every slice in R32 runs its crate's tests before
its report is written. The `make lint-rust` / `make test-unit`
targets plus per-crate `cargo test -p <crate>` remain the required
sequence (AGENTS.md §3.3 forbids `cargo test --workspace`).
