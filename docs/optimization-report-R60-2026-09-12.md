# R60 Results — 2026-09-12

Predecessor: [`optimization-report-R59-2026-09-12.md`](optimization-report-R59-2026-09-12.md)
(`1af090fc`).

R52 built a ratio gate for the performance work: every memoised path is
measured against the expression it replaced, in one process, so the factor
is machine-independent. It could only cover the paths whose "before"
expression is still in the tree — the catalog memos (R43, R51) and the
projection. The two *oldest* wins, R42's ASCII fast path and R48's
token-unit accumulator, replaced code that is gone: their claims have
lived in report prose ever since.

## What the audit found

| Finding | Evidence |
|---|---|
| **Two documented wins had no guard.** `estimate_token_count`'s ASCII fast path (R42: "13.2 µs → 60 ns") and `estimate_messages_token_count`'s unit accumulator (R48: "38.8 µs → 16.4 µs") are the framework's most-quoted numbers, and nothing would fail if either were removed | `hot_paths -- --check` covered four invariants, none of them these |
| **The "before" code no longer exists**, so the ratio cannot be expressed the way R52 expressed the others | the replaced loops were deleted in R42/R48 |
| **A reference is cheap**: both old algorithms are ~25 lines, and the current code already *pins* that its result is identical to the accumulator's naive form (`token_units_accumulate_to_the_same_estimate`) | `crates/synthia-provider/src/token_counter.rs` |

## What landed

### A. Reference implementations in the harness

```text
Reference implementations
  slow_estimate_token_count            — the pre-R42 char walk with is_cjk
  slow_estimate_messages_token_count   — one String per message, then count
```

Kept next to `check_ratios`, with a comment saying why they are *allowed*
to be dead code (`#[allow(dead_code)]` is not needed: `--check` calls
them) and what would make them stale (if the current algorithms' results
stop matching, the crate's own equivalence test fails first).

### B. Two more invariants in `--check`

| Invariant | Measured | Floor | Why that floor |
|---|---|---|---|
| `estimate_token_count (ASCII)` vs the pre-R42 char walk | **133×** (62.9 ns vs 8.4 µs) | 50× | dropping the fast path lands near 1×; the floor tolerates a busy runner |
| `estimate_messages_token_count` vs the pre-R48 String-per-message | **2.0×** (18.7 µs vs 37.2 µs) | 1.3× | the pair isolates R48, and its honest factor *is* ~2×, not 100× — a floor of 1.3 catches removal without failing on noise |

The message pair calls the **current** `estimate_token_count` inside the
old message-level loop. That is deliberate and commented: calling the
pre-R42 counter there would have re-measured R42's win inside R48's
comparison and produced a flattering 6× that means nothing.

### C. The header says what the mode is for

The module docs now explain the three kinds of entry: the table (compare
before/after on one machine), the inline "expression it replaced" pairs
(R43/R51/R59), and these reference-algorithm pairs (R42/R48) — so the next
person adding a bench knows which one to write.

## Verification

| Check | Result |
|---|---|
| `make bench-check` | **six** invariants hold (was four); output above |
| `cargo bench -p synthia-agent --bench hot_paths` | table unchanged; the two reference entries are `--check`-only (they would be noise in the table, and they measure deliberately-slow code) |
| `cargo +nightly fmt --all` / `cargo clippy --all-targets --all-features --tests --all -D warnings` | clean |
| `make ci` | green |
| `make examples` | every example plus both consumer crates |

No library code changed in this round: it is the gate that changed. That
is the point — R42 and R48 are now enforced rather than remembered.

## Known gaps after this round

The "loop-level ratios" row is gone from `docs/README.md`. Still open,
each with its reason recorded there: the builtin tools' runtime seam
(declined by design, R40), the `Timer` seam (R39), `GroupedRegistry`'s
residual name clones (R51), `cargo-deny` in CI (needs the binary and an
advisory fetch, R41), a doctest gate over `SEAMS.md` (wants rustdoc JSON,
R47), the projection's remaining per-request schema clone (re-scoped by
R59 — the cheap 80 % is done), an in-tree LLM-judge scorer and a per-run
strategy override over HTTP (features, R50), and the two tracked lockfiles
in `synthia-web/` (a maintainer workflow decision, R57).
