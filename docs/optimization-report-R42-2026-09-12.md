# R42 Results — 2026-09-12

Predecessor: [`optimization-report-R41-2026-09-12.md`](optimization-report-R41-2026-09-12.md)
(`55f8cca1`).

The docs claimed performance — "cheap heuristic", "no per-call
allocation in the hot paths", "chunked full-output retention" — and
nothing measured any of it. R42 builds the measurement, and the
measurement immediately found two problems that were worse than
anything the harness was built to watch.

## What the audit found

| Finding | Evidence |
|---|---|
| **No benchmarks at all.** A performance claim could regress silently in any PR; the only feedback loop was "someone notices" | no `benches/` anywhere in the workspace |
| **`TruncatingContextManager::prepare` was quadratic in the history length.** After every dropped user/assistant pair it re-estimated the entire remaining tail — walking every message and re-serialising every tool call's JSON | `context_manager.rs`: `state.estimated_tokens = self.estimate_tokens(&tail)` inside the pairwise-drop loop |
| **`estimate_token_count` decoded every character to classify it**, then compared it against twelve CJK ranges — for text that is almost always pure ASCII | `crates/synthia-core/src/token.rs`: `for ch in text.chars() { if is_cjk(ch) { … } }` |
| The cost was invisible because nothing called the estimator on a realistic history | `hot_paths` before/after, below |

## What landed

### A. `crates/synthia-agent/benches/hot_paths.rs` — `make bench`

A dependency-free harness (`harness = false`, no criterion) with nine
measured entries, each one a function the loop calls per iteration or
per streamed delta: token estimation (single text and 100-message
history), `BlockAssembler` folding 400 deltas, `parse_tool_input` on
strict and malformed 4 KB JSON, schema validation, tool-surface
projection over 40 descriptors, context truncation with and without
eviction, and UTF-8-safe truncation of a 200 KB buffer. Fixtures are
built once outside the loops; `std::hint::black_box` keeps the
optimiser honest; the harness is compiled by `clippy --all-targets` in
CI, so it cannot rot.

### B. The truncation loop is linear

`estimate_messages_token_count` is a `.map(…).sum()` over messages, so
the tail's total can be maintained by *subtracting* a dropped message's
own estimate instead of re-summing the survivors. The loop now does
that — and keeps the entry condition on the whole-list estimate, which
is deliberate (pinned by `truncating_never_drops_system_messages`: a
system prompt that alone exceeds the window must still let the tail be
evicted rather than short-circuiting before anything is dropped).

### C. The estimator's ASCII fast path

```rust
if text.is_ascii() {
    let text_tokens = (text.len() as f64 / 4.0) as usize;
    let overhead = (text_tokens as f64 * 0.05) as usize;
    return text_tokens + overhead;
}
```

For ASCII, `ascii_count == text.len()` and `cjk_count == 0`, so this is
the *same formula* with the loop skipped — `is_ascii` is SIMD and
answers "is there any multi-byte character, and any CJK?" in one pass.
Non-ASCII input takes the original loop verbatim. The unit tests that
pin the formula (ASCII, CJK, mixed, emoji, every `is_cjk` range bound)
all pass unchanged.

## Measured (this machine, `cargo bench -p synthia-agent --bench hot_paths`)

| Benchmark | Before | After | Change |
|---|---|---|---|
| `estimate_token_count` (4 KB ASCII) | 13 191 ns | **59.8 ns** | **220×** |
| `estimate_messages_token_count` (100 msgs) | 197 092 ns | **29 233 ns** | **6.7×** |
| `TruncatingContextManager` (200 msgs, over window) | **34 189 167 ns** | **365 002 ns** | **94×** |
| `TruncatingContextManager` (200 msgs, no eviction) | 282 401 ns | **80 146 ns** | **3.5×** |
| `BlockAssembler` fold (400 deltas) | 73 235 ns | 60 444 ns | 1.2× (calls the estimator) |
| `parse_tool_input` (4 KB strict) | 68 864 ns | 57 779 ns | 1.2× |
| `repair_json` (4 KB defective) | 17 129 ns | 15 847 ns | 1.1× |
| `validate_against_schema` (20 fields) | 1 145 ns | 936 ns | 1.2× |
| `project_tool_definitions` (40 tools) | 24 847 ns | 21 140 ns | 1.2× |
| `cap_to_char_boundary` (200 KB → 100 KB) | 5 270 ns | 3 434 ns | 1.5× |

The headline: **the loop's per-iteration context check went from 34 ms
to 0.37 ms**. At 34 ms per iteration on a long conversation, the
truncating manager was costing more than the network round trip it was
preparing for.

The remaining 29 µs for 100 messages is one `String` allocation and
concatenation per message inside `estimate_messages_token_count`;
removing it means exposing a count-accumulator in `synthia-core` and
is recorded as a follow-up rather than smuggled into this round.

## Verification

| Check | Result |
|---|---|
| `cargo +nightly fmt --all` / `--check` | clean |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings, 0 errors (bench target included) |
| Per-crate tests | **2585 passed / 0 failed** — identical totals to R41, so the optimisation is behaviour-preserving |
| `cargo test -p synthia-context` | 95 passed / 0 failed, including the two tests that pin the truncation semantics |
| `make bench` | runs; table above |
| `make check-mvp-deps` / `make check-no-runtime` | green |
| Default-feature sweep of the estimator's unit tests (ASCII / CJK / mixed / emoji / 24 range bounds) | unchanged and green |

## Deferred to R43 (recorded, not dropped)

- **A count accumulator to drop the per-message `String`** in
  `estimate_messages_token_count` (~25% of the remaining estimator
  cost).
- **A runtime seam for the builtin tools** (carried from R40): `read` /
  `write` / `shell` still use `tokio::fs` / `tokio::process`.
- **A `Timer` seam** (carried from R39).
- **`synthia-server`'s `middleware/trace_context.rs`** still
  reimplements `synthia_telemetry::propagation` (carried from R37).
- **`cargo-deny` is not wired into CI** (carried from R41): `deny.toml`
  is still a placeholder.
- **The bench table is single-machine.** A `make bench` baseline
  committed to the repo would catch order-of-magnitude regressions in
  CI (where absolute numbers differ but ratios do not); deciding the
  threshold and the CI runner shape is its own round.
