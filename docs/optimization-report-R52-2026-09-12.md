# R52 Results — 2026-09-12

Predecessor: [`optimization-report-R51-2026-09-12.md`](optimization-report-R51-2026-09-12.md)
(`01d264e0`).

R42–R51 made the framework's overhead measurable and then smaller.
Nothing stopped it from coming back: `make bench` printed a table, and
nothing compared it to anything. Absolute timings cannot be gated on a
shared CI runner — but *ratios measured in one process* can.

## What the audit found

| Finding | Evidence |
|---|---|
| **The perf work was unguarded.** Seven rounds of measurements (R42–R51), each ending in a report with numbers a human reads; a regression would reappear silently, and the next reader would have to remember what the numbers used to be | no target or CI job referenced the harness |
| **Absolute nanoseconds are the wrong thing to gate on.** The same entry read 170 µs and 300 µs on the same machine minutes apart during R50/R51 (shared box, another process competing) | `ReActAgent turn` across the two runs of R51 |
| **Ratios in one process are portable.** `descriptors` vs `descriptors_cached` was 1648× and `snapshot` vs `snapshot_cached` 429× in the same run where the absolute times moved 2×; the factor barely moved, because both sides are affected by the same interference | R51's table |
| **The benchmark had no failure mode**, so the harness's own fixtures could rot (a bench that silently measures nothing still prints a plausible row) | `Bench::run` ignored its result |

## What landed

### A. `--check`: the invariants the table cannot state

```text
$ make bench-check

     invariant                                              measured
---------------------------------------------------------------------
OK   descriptors_cached vs descriptors                      1570.1x  (min 100.0x; 8.8 ns vs 13888 ns)
OK   snapshot_cached vs snapshot                             429.0x  (min 100.0x; 8.1 ns vs 3471 ns)
OK   AdaptiveRegistry::visible_tool_names vs the uncached scan   68.6x  (min  10.0x; 59.9 ns vs 4109 ns)
OK   GroupedRegistry::visible_tool_names vs the uncached scan    3.6x  (min   2.0x; 1446.1 ns vs 5190 ns)

all performance invariants hold
```

Each row is *the same question answered twice*: the memoised path and
the exact expression it replaced (the R43, R51 code, spelled out in the
harness), measured by the same min-of-5 estimator in one process. That
is what makes the factor portable and the gate honest — it fails when a
cache stops hitting or a scan is reintroduced, and it does not fail
because the runner was busy.

- `measure(iters, f)` was extracted from `Bench::run`, so the table and
  the check use one estimator rather than two that can drift.
- Floors carry deliberate headroom: the tightest margin (the group
  wrapper, 3.6×) sits at a 2.0× floor, because its remaining cost is the
  visible-name clones the `Vec<String>` signature requires. A regression
  that removes a memo lands near **1×**, far below every floor, so the
  gate is sensitive where it matters and quiet where noise lives.
- On failure the message names the remedy (`make bench` for the table,
  and "if the change is intentional, adjust the minimum factor … and say
  why in the commit") — a gate that tells the next person how to
  disagree with it on purpose.

### B. A fourth CI job

`.github/workflows/rust-quality.yml` gained a `bench` job running
`make bench-check`. It is **not** folded into `make ci`: the ratio check
compiles release binaries, and the fast gate is meant to stay fast. The
README's CI description and the `make` command list were updated in the
same change, so the documented job count and the workflow cannot drift.

## Verification

| Check | Result |
|---|---|
| `make bench-check` | all four invariants hold (output above) |
| `cargo bench -p synthia-agent --bench hot_paths` | table unchanged; every entry within noise of R51 |
| `cargo +nightly fmt --all` / `make fmt-check` | clean |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings, 0 errors |
| `make ci` | green (unchanged — the new gate lives in its own job) |
| `yaml.safe_load(.github/workflows/rust-quality.yml)` | parses; jobs `gates`, `tests`, `examples`, `bench` |
| Full per-crate sweep | unchanged by this round (no library code touched): see R51 |

The round touches no library code, so no behaviour changed; what changed
is that the previous four rounds' claims now fail a build when they stop
being true.

## Deferred to R53 (recorded, not dropped)

- **Ratios for the provider/loop half.** The four invariants all cover
  tool-catalog memos. The loop's own pairs — `estimate_token_count`
  (ASCII fast path), `estimate_messages_token_count` (the `TokenUnits`
  accumulator, R48) — have no "before" expression left in the tree to
  compare against, so they need a reference implementation in the
  harness (a naive port) to become enforceable. Worth doing for the two
  hottest entries, and only those.
- **A "bench did real work" assertion.** A fixture that silently
  degenerates (an empty registry, a zero-length transcript) still prints
  a plausible row; `--check` partially covers this, since a degenerate
  fixture collapses a ratio.
- Carried: the projection's JSON clones (`Arc<Value>` schema, 21.5 µs per
  request — needs `serde/rc`), `GroupedRegistry`'s residual clones,
  `cargo-deny` in CI (R41), a doctest gate over `SEAMS.md` (R47), the
  builtin tools' runtime seam (R40, declined by design), the `Timer`
  seam (R39), an in-tree LLM-judge scorer and a per-run strategy
  override over HTTP (R50).
