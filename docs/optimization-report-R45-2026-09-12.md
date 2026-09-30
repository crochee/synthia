# R45 Results — 2026-09-12

Predecessor: [`optimization-report-R44-2026-09-12.md`](optimization-report-R44-2026-09-12.md)
(`f7b7f6f3`).

R36 gave the framework `synthia_core::Clock` and R40 wired it into the
agent, but the rule behind it — AGENTS.md §3.8, "new code must not call
`chrono::Utc::now()`" — still had production hold-outs. R45 finished the
job, and one of the hold-outs turned out to be a real bug.

## What the audit found

| Finding | Evidence |
|---|---|
| **Every timestamped component read the wall clock itself.** The attachment store, both memory tiers, the eval report, the session's operation snapshots, the workflow journal, the provider's retry-after arithmetic, and four sites in the server's session log all called `chrono::Utc::now()` directly | an audit over `crates/*/src`: 11 production call sites across 8 crates |
| **Managed spill files collided.** `spill_to_file` named files `{session}-{tool}-{%Y%m%d%H%M%S}.txt` — second resolution, no uniqueness — so a second spill in the same second overwrote the first, while the truncation marker in the conversation still promised the full text | `crates/synthia-tool/src/truncate/bound_output.rs` |
| Nothing enforced the rule, so it could only be re-broken silently | no gate over `Utc::now()` |

## What landed

### A. One clock per process, one seam per component

Every direct read now goes through `synthia_core::Clock`, and every
public constructor that "stamps now" gained an explicit-`now` sibling so
a caller can be deterministic without a global:

| Component | Added | Default path |
|---|---|---|
| attachments | `ImageAttachmentRef::from_bytes_at`, `AttachmentStore::with_clock`, `AttachmentStore::clock()` | the **store** owns `saved_at`; a byte backend has no business deciding what "now" is |
| memory | `MemoryEntry::at`, `SqliteMemory::with_clock` | `sessions.created_at` comes from the store's clock |
| eval | `EvalRunner::with_clock` | the report's `generated_at` |
| session | `OperationSnapshot::new_at` | `OperationSnapshot::new` |
| workflow | `JournalEntry::{success_at, failure_at}` | journal `recorded_at` |
| provider | `parse_retry_after_at` | `parse_retry_after` (the integer-seconds form ignores `now` entirely) |
| server | `RunDependencies::with_clock`, `AppState.clock` | the session log's `ts`, operation snapshots, the hash-only attachment response, and the MCP supervisor's `tick` all read the one clock `AppState` builds at boot |

### B. The spill filename stops colliding

```rust
// A ULID, not a wall-clock stamp: it is the workspace's id vocabulary
// (`synthia_core::IdGen`), it sorts by creation time anyway, and it
// cannot collide — two spills in the same second used to share a
// filename and the second overwrote the first.
let suffix = synthia_core::IdGen::next_id(&synthia_core::SharedIdGen::ulid());
let filename = format!("{}-{}-{}.txt", session_id, tool_name, suffix);
```

A timestamp used as a unique key is a bug waiting for a fast machine;
the workspace already ships the right tool for it.

### C. A ratchet so it cannot come back

`make check-clock` counts `chrono::Utc::now()` occurrences in
`crates/*/src` (excluding `clock.rs`, which *is* the implementation) and
fails when the count grows. It is part of `make ci`, so CI enforces it:

```
OK: 25 chrono::Utc::now() occurrence(s) in library src (baseline 25;
    doc comments and tests only)
```

`make clock-audit` prints every occurrence (file, line, text) for
review — currently 25, all inside doc comments or `#[cfg(test)]` modules.
The baseline can only be lowered, which the Makefile comment states.

## Verification

| Check | Result |
|---|---|
| `cargo +nightly fmt --all` / `--check` | clean |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings, 0 errors |
| Per-crate tests | **2588 passed / 0 failed** (R43+44: 2585; +2 the attachment clock test and the tool cache tests, +1 net elsewhere), plus `synthia-context --features sqlite` 103 |
| Crates touched | attachments 16, context 95 (103 with `sqlite`), eval 36, session 137, workflow 65, provider 744, tool 309, server 437 — all green |
| `make clock-audit` | 25 occurrences, every one audited: doc comments + in-crate tests |
| `make ci` | green, now including `check-clock` |
| `make examples` / consumer proofs | green |

## Deliberate decisions

- **`SystemClock::now()` inside the `_at`-less constructors.** A helper
  like `MemoryEntry::now` has no injection point by construction; it
  routes through the `Clock` *trait's* production impl rather than
  `chrono::Utc::now()`, so replacing the clock is one type change and
  the audit sees no direct wall-clock read. Callers that need
  determinism use the `_at` sibling.
- **`synthia-session` gained a `synthia-core` dependency** for `Clock`
  (the direction the workspace mandates: `synthia-*` → `synthia-core`).
- **`cleanup_managed_files` still uses `SystemTime::now()`** to compare
  file mtimes — that is a filesystem-timestamp comparison, the one
  documented-correct `std::time` use (R24), not a domain wall-clock read.

## Deferred to R46 (recorded, not dropped)

- **A count accumulator to drop the per-message `String`** in
  `estimate_messages_token_count` (carried from R42).
- **A runtime seam for the builtin tools** (carried from R40).
- **A `Timer` seam** (carried from R39).
- **`cargo-deny` in CI** (carried from R41) — `deny.toml` is empty and
  the binary is unavailable locally, so wiring it would be an
  unverifiable gate.
- **The clock ratchet counts tests too.** A stricter gate would parse
  `#[cfg(test)]` blocks out before counting; the ratchet plus
  `clock-audit` was judged enough for now, and the parser is its own
  small project.
