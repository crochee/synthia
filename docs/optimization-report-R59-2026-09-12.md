# R59 Results — 2026-09-12

Predecessor: [`optimization-report-R58-2026-09-12.md`](optimization-report-R58-2026-09-12.md)
(`d6993a8f`).

The largest single item on the loop's per-iteration path, measured since
R42 and re-confirmed every round: the model-facing tool list. R51
proposed sharing the schemas (`Arc<serde_json::Value>`) — this round
found a cheaper way to the same win, and with no public-type change.

## What the audit found

| Finding | Evidence |
|---|---|
| **The projection runs once per iteration, but almost never changes between iterations.** Its inputs: the registry (version-keyed, and `descriptors_cached` already exploits that), the deployment surface, this agent's restriction, whether delegation is on, and the set of already-called `Deferred` tools. Everything but the last is fixed for the loop's lifetime | `compose_tool_definitions`, `bench: project_tool_definitions (40 tools)` 21.5 µs |
| **The result was thrown away each time**: `tool_definitions` returned an owned `Vec<ToolDefinition>`, and the request wrapped it in a *fresh* `Arc` (`tools: Arc::new(tools)`) — destroying the sharing the request type already supported | `sample_once`: `tools: Arc::new(tools)` |
| **The wasted work is proportional to the catalog**: 21.5 µs at 40 tools, ~12 % of a 170 µs scripted turn; at 5 tools it is invisible, which is why it survived this long | the bench pair (R42–R58) |
| The R43 short-circuit (skip the transcript scan when nothing is `Deferred`) lived *inside* the projection, so anything that wanted to key on the projection would have had to pay that scan first | `compose_tool_definitions` |

## What landed

### A. `ToolProjectionMemo`, per run

```rust
struct ToolProjectionMemo {
    registry_version: u64,   // every registration / removal / hidden / exposure change
    promoted: Vec<String>,   // sorted already-called Deferred names
    defs: Arc<Vec<ToolDefinition>>,
}
```

`ReActLoop::tool_definitions` looks the memo up, and on a miss composes,
stores, and returns the shared `Arc`. The caller hands that `Arc` straight
to the request (`tools` is already `Arc<Vec<ToolDefinition>>`), so a hit is
a mutex read plus a refcount bump — no projection, no deep clone.

The key is the *two* things that can change mid-run; keying on the sorted
promoted set (rather than the `HashSet` iteration order) makes two orderings
of the same transcript state hit one entry.

### B. The short-circuit moved where it is needed

`promoted_tool_names` owns the "no `Deferred` tool in the catalog → no
transcript scan" decision, shared by the memo path and the one-shot
`ReActAgent::projected_tool_definitions`. Without that move, *keying* the
memo would have cost the O(messages × parts) scan on every iteration — i.e.
the optimisation would have paid for itself out of the work it was trying
to avoid.

### C. It cannot serve a stale projection

Two tests assert the contract through the provider's captured `Arc`s —
what the wire actually carries, not an internal pointer:

| Test | Assertion |
|---|---|
| `repeated_iterations_share_one_tool_definition_allocation` | one tool-using turn (two requests) → `Arc::ptr_eq(captured[0], captured[1])`, and the catalog is still the full two tools |
| `deferred_promotion_invalidates_the_projection_memo` | a `Deferred` tool called mid-turn → the two requests' Arcs differ, and the second carries the *real* schema instead of the permissive placeholder |

## Measurements

Interleaved A/B on one machine: the bench built from `HEAD` (R58, without
the memo) and this tree (with it), alternating, so drift hits both sides.
The unchanged `project_tool_definitions` entry is the control — it tracks
together within each pair, which is what makes the turn comparison
readable.

| Pair | `project_tool_definitions` (control) | `ReActAgent turn` before | after | Δ |
|---|---|---|---|---|
| 1 | 46.3 / 49.9 µs | 360.9 µs | 319.1 µs | **−41.8 µs (−11.6 %)** |
| 2 | 24.7 / 25.2 µs | 274.1 µs | 238.2 µs | **−35.9 µs (−13.1 %)** |

The saving is one full projection per turn (a turn with one tool call
issues two requests: miss, then hit), which is what the mechanism predicts.
The box is noisy — the control entry itself swings ~2× across pairs — so
the *pairs* are the evidence, not the absolute numbers (R51 measured the
same turn at 170 µs on a quieter machine).

## Verification

| Check | Result |
|---|---|
| `cargo test -p synthia-agent` | **330 passed / 0 failed** (328 before: +2) |
| Full per-crate sweep (19 crates) | **2614 passed / 0 failed** (corrected in R61: the total in this row was hand-summed 10 high; the per-crate numbers above are the sweep output); `synthia-context --features sqlite` 103 |
| `cargo +nightly fmt --all`, `cargo clippy --all-targets --all-features --tests --all -D warnings` | clean |
| `make ci` | green (fmt, clippy, **rustdoc `-D warnings`**, dependency invariants, clock ratchet) |
| `make bench-check` | all four ratio invariants hold |
| `make examples` | every example plus both consumer crates |
| Interleaved A/B | table above |
| Worktree hygiene | the baseline tree was a `git worktree` at `HEAD`, removed after the measurement |

## What this changes about R51's deferred item

`Arc<serde_json::Value>` schemas were proposed in R51 to kill the per-request
deep clone. The memo captured the **repeated** half — the projection itself
— leaving one deep clone per request, on the way into the adapter's wire
body. That clone is now dominated by the request's own serialization, and
the change would cost two public-type changes plus a workspace-wide
`serde/rc` feature. The gap entry in `docs/README.md` is re-scoped
accordingly rather than silently carried: the cheap 80 % is done, and the
remaining 20 % is no longer worth its blast radius.

## Known gaps after this round

Unchanged from R58 except that re-scoped row: per-agent tool restriction is
done (R58); still open are the builtin tools' runtime seam (declined by
design), the `Timer` seam, `GroupedRegistry`'s residual clones,
loop-level benchmark ratios, `cargo-deny` in CI, a doctest gate over
`SEAMS.md`, an in-tree LLM-judge scorer, a per-run strategy override over
HTTP, and the two tracked lockfiles in `synthia-web/`.
