# R43 Results — 2026-09-12

Predecessor: [`optimization-report-R42-2026-09-12.md`](optimization-report-R42-2026-09-12.md)
(`aa9105dc`).

R42 built the harness and fixed the two quadratic paths it found.
R43 extended it with the number a user actually feels — one whole turn —
and that number made the per-iteration preparation visible.

## What the audit found

| Finding | Evidence |
|---|---|
| **The loop deep-cloned every tool's JSON schema on every iteration.** `compose_tool_definitions` called `registry.descriptors()`, which builds owned `ToolDescriptor`s — including `parameters: serde_json::Value` — then handed them to a projection that clones them again | `crates/synthia-agent/src/agent/re_act.rs`; measured at **13 299 ns** per iteration for 40 tools |
| **The transcript scan ran even when nothing needed it.** `called_tool_names` is O(messages × parts) and exists solely to promote `Deferred` tools; a catalog with no deferred tool paid it anyway | **6 018 ns** on a 200-message transcript, once per iteration |
| Combined, the per-iteration preparation was ~45 µs of the ~352 µs turn | 13% of the turn spent rebuilding a list that changes only when the catalog or the `called` set does |

## What landed

### A. `ToolRegistry::descriptors_cached()`

The registry already stamps a monotonic `version` on every
register/unregister (the server's definition cache keys off it). The
new accessor memoises the descriptor list against that version and
hands out an `Arc`, so the deep clone of every schema happens once per
*catalog change* instead of once per request:

| Benchmark | Before | After |
|---|---|---|
| `ToolRegistry::descriptors` (40 tools) | 13 299 ns | — (still available, uncached) |
| `ToolRegistry::descriptors_cached` (40 tools, warm) | — | **8.5 ns** |

**1560×** on the hot path. The memo is invalidated by the existing
version bump, and two new tests pin the two ways it could go wrong: a
tool registered or unregistered after a cached read must appear or
disappear on the next call (otherwise the model is advertised a stale
catalog), and a cloned registry must keep a usable memo without
leaking changes back to the original.

### B. The transcript scan is skipped when nothing is deferred

`project_tool_definitions` reads the `called` set only in the
`Deferred` branch. The loop now checks whether any descriptor is
deferred before scanning:

```rust
let called = if descriptors.iter().any(|d| d.exposure == ToolExposure::Deferred) {
    synthia_tool::called_tool_names(messages)
} else {
    HashSet::new()
};
```

Same output (the set is unused when nothing is deferred), no scan on
the common catalog.

### C. The end-to-end number, and what it moved

The bench gained a complete-turn entry: a scripted provider that asks
for one tool and then answers, a 40-tool registry, and a spawner that
runs on its own thread (`Spawner`, not tokio) so the measurement is the
framework's own work — context preparation, tool dispatch, the event
stream, the typed sink.

| | R42 baseline | R43 |
|---|---|---|
| **`ReActAgent` turn (scripted, 1 tool call)** | **352 158 ns** | **235 519 ns** |
| per-iteration tool-list preparation | ~45 µs | ~21 µs |

**One third off a turn**, from two changes that do not alter a single
output byte.

*Run-to-run variance on this machine is ±20% for the turn entry (it
includes thread spawn and scheduling), so treat 236 µs and 352 µs as
the two ends of a distribution, not exact values — the ratio is what
the change bought. The `descriptors_cached` and token-estimator entries
are stable to within a few percent.*

## Full table (`make bench`)

| Benchmark | per op | note |
|---|---|---|
| `estimate_token_count` (4 KB ASCII) | 64.7 ns | 220× vs pre-R42 |
| `estimate_messages_token_count` (100 msgs) | 33 265 ns | 6.7× vs pre-R42 |
| `BlockAssembler` fold (400 deltas) | 64 191 ns | per-delta cost |
| `parse_tool_input` (4 KB strict) | 58 404 ns | happy path |
| `repair_json` (4 KB defective) | 17 128 ns | salvage path |
| `validate_against_schema` (20 fields) | 942 ns | structured output |
| `project_tool_definitions` (40 tools) | 21 247 ns | per request |
| `called_tool_names` (200-message transcript) | 6 018 ns | only when deferred tools exist |
| `ToolRegistry::descriptors` (40 tools) | 13 299 ns | uncached |
| `ToolRegistry::descriptors_cached` (40 tools) | **8.5 ns** | what the loop uses |
| `TruncatingContextManager` (200 msgs, over window) | 372 727 ns | 94× vs pre-R42 |
| `TruncatingContextManager` (200 msgs, no eviction) | 84 555 ns | fast path |
| **`ReActAgent` turn (scripted, 1 tool call)** | **235 519 ns** | end to end |
| `cap_to_char_boundary` (200 KB → 100 KB) | 3 535 ns | clone + UTF-8 safe cut |

## Verification

| Check | Result |
|---|---|
| `cargo +nightly fmt --all` / `--check` | clean |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings, 0 errors |
| `cargo test -p synthia-tool` | **309 passed / 0 failed** (307 + the two cache-invalidation tests) |
| `cargo test -p synthia-agent` | 306 passed / 0 failed |
| `cargo test -p synthia-server` | 437 passed / 0 failed |
| `make bench` | table above |
| `make ci` / `make check-no-runtime` | green |

## Deferred to R44 (recorded, not dropped)

- **The projection still clones each schema once per call**
  (`definition_for` → `descriptor.parameters.clone()`). With the
  descriptor memo in place the remaining cost is ~21 µs per iteration
  for 40 tools; caching the *projected* list against
  `(registry version, deferred-set, visible-set)` is the next step, but
  the deferred set is transcript-dependent, so the key needs care.
- **A count accumulator to drop the per-message `String`** in
  `estimate_messages_token_count` (carried from R42, ~25% of that
  path).
- **A runtime seam for the builtin tools** (carried from R40).
- **A `Timer` seam** (carried from R39).
- **`synthia-server`'s `middleware/trace_context.rs`** still
  reimplements `synthia_telemetry::propagation` (carried from R37).
- **`cargo-deny` is still not wired into CI** (carried from R41).
