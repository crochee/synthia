# R51 Results — 2026-09-12

Predecessor: [`optimization-report-R50-2026-09-12.md`](optimization-report-R50-2026-09-12.md)
(`99c57ce9`).

A performance round, driven by the benchmark harness rather than by
suspicion: R42–R48 memoised the *projection* inputs the loop rebuilds
every iteration, and this round found the sibling path — the catalog
every "what should the model be told about?" consumer starts from — was
still a full scan with two `String` clones per tool, computed for the
whole registry and then filtered down.

## What the audit found

| Finding | Evidence (bench, 40 tools, one run) |
|---|---|
| **The catalog was rebuilt per call.** `ToolRegistry::snapshot()` clones `name` + `description` for every tool and sorts by name — 3.90 µs for 40 tools — while `descriptors()` (the heavier projection input, all schemas) is memoised to **8 ns**. One half of the pair had a memo; the other did not | `ToolRegistry::snapshot` vs `ToolRegistry::descriptors_cached` |
| **The tier cap paid for tools it hides.** `AdaptiveRegistry::visible_tool_names` snapshotted the registry and then `.take(caps)`: a `Small` deployment (cap 5) cloned 40 descriptions to keep 5 names — **3.84 µs to answer "these five"** | `AdaptiveRegistry::visible_tool_names (40→5)` |
| **The same shape in two more consumers.** `GroupedRegistry::visible_tool_names` (5.15 µs) and `RestrictedRegistry::visible_names` both snapshotted everything, then filtered; `visible_metadata_snapshots` on both wrappers did too | `GroupedRegistry::visible_tool_names (40 tools)` |
| **Nothing in the harness measured any of it**, so the cost was invisible: the loop's own bench covers `descriptors_cached`, not the read-side catalog the *consumer-facing* wrappers use | `crates/synthia-agent/benches/hot_paths.rs`, before this round |

The wrappers are the framework's "lego" surface — the tier cap is what
`Steering::for_tier` + `AdaptiveRegistry` assemble for a small model, and
the groups are what `[tools]` config and the workflow/subagent layers
build on. Making them scale with *what they advertise* rather than with
*what the registry holds* is the same class of fix as R43's
`descriptors_cached`, applied to the other half of the pair.

## What landed

### A. `ToolRegistry::snapshot_cached()`

The catalog behind the same version-keyed memo as `descriptors_cached`:
`(registry version, Arc<Vec<ToolMetadataSnapshot>>)`, recomputed only
when the version moves. `snapshot()` stays for callers that want a
fresh owned `Vec` (and is documented as the uncached read).

The version is bumped by registration, removal **and `set_hidden`**, so
the privacy contract holds: a tool hidden after the catalog was cached
leaves it on the very next read, and unhiding restores it. Two tests pin
exactly that (memo reuse until a change; hidden flag followed in both
directions) — a stale catalog would advertise a tool the model cannot
call, or hide one it can.

### B. Consumers clone only what they keep

| Consumer | Before | After |
|---|---|---|
| `AdaptiveRegistry::visible_tool_names` | `snapshot()` (40 tools) → `take(5)` → 5 names | `snapshot_cached()` → `take(5)` → 5 name clones |
| `AdaptiveRegistry::visible_metadata_snapshots` | `snapshot()` → `take(cap)` | `snapshot_cached()` → `take(cap)` → `cloned()` |
| `GroupedRegistry::visible_tool_names` | `snapshot()` → filter → names | `snapshot_cached()` → filter → visible names |
| `GroupedRegistry::visible_metadata_snapshots` | `snapshot()` → filter | `snapshot_cached()` → filter → `cloned()` |
| `RestrictedRegistry::visible_names` | `snapshot()` (twice, branch on empty) → names | `snapshot_cached()` → filter → visible names |

Behaviour is unchanged in every case — the same names, the same order,
the same hidden-tool exclusion; only the work per call changed.

## Measurements

Same process, same machine, one run — each consumer is measured next to
the expression it replaced, so the pair does not depend on comparing
across machine states (the box is shared; R48's numbers were taken on a
quieter machine and some entries read 2× higher in an earlier pass of
this round).

| Entry | Before | After | Change |
|---|---|---|---|
| `ToolRegistry::snapshot` (40 tools) | 3.90 µs | — | unchanged (still the uncached read) |
| `ToolRegistry::snapshot_cached` (40 tools) | — | **8.0 ns** | ~490× vs `snapshot` |
| `AdaptiveRegistry::visible_tool_names` (40→5) | 3.84 µs | **56.3 ns** | **68×** |
| `GroupedRegistry::visible_tool_names` (40 tools) | 5.15 µs | **1.49 µs** | **3.5×** |
| `ToolRegistry::descriptors` → `descriptors_cached` | 14.0 µs | 8.4 ns | R43's memo, re-confirmed in the same run |
| `ReActAgent turn` (scripted, 1 tool call) | — | 170.8 µs | unchanged: the loop uses the memoised descriptors, not the catalog wrappers |

The grouped path's remainder (1.49 µs) is the 20 visible `String` clones
the `Vec<String>` signature requires; the tier path's remainder (56 ns)
is the 5 it keeps.

## Verification

| Check | Result |
|---|---|
| `cargo +nightly fmt --all` / `make fmt-check` | clean |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings, 0 errors |
| `cargo test -p synthia-tool` | **311 passed / 0 failed** (309 before: +2 memo-contract tests) |
| Full per-crate sweep (19 crates) | **2614 passed / 0 failed** — core 108, telemetry 36, provider 744, context 95 (103 with `sqlite`), tool 311, session 137, steering 74, skill 56, agent 325, attachment 16, mcp 52, rag 38, scheduler 14, macros 31, eval 36, workflow 65, test-support 18, facade 17, server 441 |
| `cargo bench -p synthia-agent --bench hot_paths` | table above; every pre-existing entry within noise of R48 |
| `make ci` | green — fmt-check, clippy, MVP dependency set, runtime-free crates, clock ratchet (18 code calls) |
| `make examples` | every example plus both consumer crates (`MVP-OK`) |

Behaviour is unchanged for every existing user: the same catalog, the
same order, the same exclusions.

## Deferred to R52 (recorded, not dropped)

- **The projection's JSON clones.** `project_tool_definitions` costs
  21.5 µs per request for 40 tools (12.6 % of a 170 µs turn) because it
  deep-clones each visible tool's `serde_json::Value` schema into the
  wire `ToolDefinition`. Passing the schema as a shared handle
  (`Arc<Value>`) would make it a refcount bump, but it changes the
  provider's wire type and needs `serde/rc` workspace-wide — worth doing
  when a deployment's *tool count* (not its history) is the pressure
  point, with a benchmark first.
- **`GroupedRegistry`'s residual clone.** Its group state changes
  independently of the registry version, so memoising its answer needs a
  group-state version counter. 1.49 µs per call is not yet worth the
  bookkeeping — revisit if a consumer calls it per streamed delta.
- **A benchmark baseline in CI** (carried since R48): the harness prints
  a table, but nothing fails on a regression. A ratio-based guard
  (`snapshot_cached` must stay ≥100× faster than `snapshot`) would be
  machine-independent where absolute timings are not.
- Carried: the builtin tools' runtime seam (R40 — declined by design,
  restated in `SEAMS.md`), the `Timer` seam (R39), `cargo-deny` in CI
  (R41), a doctest gate over `SEAMS.md` (R47), an in-tree LLM-judge
  scorer and a per-run strategy override over HTTP (R50).
