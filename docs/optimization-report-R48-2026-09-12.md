# R48 Results — 2026-09-12

Predecessor: [`optimization-report-R47-2026-09-12.md`](optimization-report-R47-2026-09-12.md)
(`86d348bc`).

R42's harness put a number on `estimate_messages_token_count` — 197 µs
for a 100-message history — and every round since has paid it once per
iteration (the context-window check) plus once per dropped pair. R48
removes the reason it was expensive.

## What the audit found

| Finding | Evidence |
|---|---|
| **The estimator built a `String` per message.** It concatenated every text / reasoning / tool-result part (plus the serialised tool-call JSON) into one buffer and measured that — an allocation and a copy per message, on a path that runs every iteration | `crates/synthia-provider/src/token_counter.rs`; 197 µs / 100 messages in R42's first measurement |
| The heuristic itself is two separable steps (count units, then convert), so the concatenation was never necessary — only the *conversion* needs the whole message | `estimate_token_count`'s formula |
| Nothing pinned the equivalence, so a future "optimisation" of the estimator could change compaction thresholds silently | no test related accumulation to the one-shot result |

## What landed

### A. `synthia_core::token::TokenUnits`

The heuristic split in two:

```rust
pub struct TokenUnits { pub ascii_bytes: usize, pub cjk_chars: usize }

impl TokenUnits {
    pub fn of(text: &str) -> Self;      // counts, no allocation (ASCII fast path intact)
    pub fn add(&mut self, other: Self); // fold another piece's units
    pub fn tokens(&self) -> usize;      // the same arithmetic as before
}
```

`estimate_token_count(text)` is now `TokenUnits::of(text).tokens()` — one
implementation of the formula instead of two paths that could drift.

### B. The message estimator accumulates

`estimate_messages_token_count` folds each part's units and converts
**once per message**. The serialised tool-call JSON is still built
(measuring JSON requires the JSON), but the per-message text buffer is
gone. The result is identical by construction: the counts are additive
across parts, and the conversion is the same arithmetic applied to the
same totals. A new test pins that for empty / ASCII / CJK / mixed /
emoji inputs *and* for a four-part message measured both ways.

### C. Measured on the same harness (min-of-5)

| Benchmark | R44 | R48 | Change |
|---|---|---|---|
| `estimate_messages_token_count` (100 msgs) | 38 759 ns | **16 407 ns** | **2.4×** |
| `TruncatingContextManager` (200 msgs, no eviction) | 118 439 ns | **71 152 ns** | **1.7×** |
| `estimate_token_count` (4 KB ASCII) | 71.3 ns | 57.2 ns | 1.2× |
| **`ReActAgent` turn (scripted, 1 tool call)** | 261 578 ns | **170 006 ns** | **1.54×** |

Against R42's first measurement of the same turn — **352 158 ns** — the
loop is now **2.07× faster**, from four changes across R42 / R43 / R48:

1. the truncation loop stopped re-estimating the whole tail per drop
   (34.2 ms → 0.37 ms on a 200-message history that does not fit);
2. `estimate_token_count` got the ASCII fast path (13.2 µs → 60 ns on
   4 KB);
3. the loop stopped deep-cloning every tool schema per iteration
   (13 299 ns → 8.5 ns) and stopped scanning the transcript when no
   tool is deferred;
4. the message estimator stopped building a `String` per message.

None of the four changes a single output byte; the whole suite is the
evidence (2588 tests, unchanged counts).

## Verification

| Check | Result |
|---|---|
| `cargo +nightly fmt --all` / `--check` | clean |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings, 0 errors |
| `cargo test -p synthia-core` | 108 passed (107 + the accumulation-equivalence test) |
| `cargo test -p synthia-provider` | 744 passed / 0 failed |
| `cargo test -p synthia-context` / `-p synthia-agent` / `-p synthia-tool` / `-p synthia-server` | 95 / 306 / 309 / 437, all green |
| `make bench` | table above |
| `make ci` / `make examples` | green |

## Deferred to R49 (recorded, not dropped)

- **A runtime seam for the builtin tools** (carried from R40) — the last
  item on the runtime-independence list, and a deliberate non-goal for
  now (see [`SEAMS.md`](SEAMS.md) §2 and the R39 report).
- **A `Timer` seam** (carried from R39).
- **`cargo-deny` in CI** (carried from R41) — `deny.toml` is empty and
  the binary is unavailable locally, so wiring it would be an
  unverifiable gate.
- **A doctest gate over `SEAMS.md`** (carried from R47).
- **A benchmark baseline in CI**: ratios are comparable across runners
  even when absolute numbers are not, but choosing the threshold and the
  runner shape is its own round.
