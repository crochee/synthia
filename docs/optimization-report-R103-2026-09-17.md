# Optimization report R103 — 2026-09-17

## What the audit found

R102 closed the strategy files. The follow-up audit swept the
remaining `synthia-agent` modules for the R89 "test blocks out of
large production files" pattern and found the `events` module
holding two of them:

| File | total | inline tests | production |
|---|---|---|---|
| `events/event_enum.rs` | 516 | 331 | 185 |
| `events/system_event.rs` | 382 | 227 | 155 |

`event_enum.rs` interleaved the 4-variant `AgentEvent` enum +
its 7 convenience constructors with a 331-line test module (7
`make_*` fixtures + 16 tests: the `kind()` label matrix, the
`is_durable()` classification matrix, `serialized_size()`, and
constructor round-outs). `system_event.rs` interleaved the
9-variant enum + `WarningKind` + `SteeringSource` with a
227-line test module (the snake_case wire-tag matrix, `kind()`
labels, and round-trip / rejection tests).

The two remaining event files were already fine:
`agent_meta.rs` (212 — 93 prod + 119 inline tests) and
`reasons.rs` (160 — 35 prod + 125 inline tests) sit well below
the ~300-line inline block that motivated R100–R102; their tests
stay inline.

## What landed

Pure code motion in the `foo/mod.rs` + `foo/tests.rs` layout
(the same shape `re_act/`, `strategy/`, `builder/`, `team/`,
`best_of_n/`, `cot/`, and `prompt/` already use). Each former
`events/<name>.rs` file became `events/<name>/mod.rs` (the
production type) + `events/<name>/tests.rs` (its tests):

```
event_enum/mod.rs     186 lines   AgentEvent + kind/is_durable/serialized_size + 7 ctors
event_enum/tests.rs   333 lines   make_* fixtures + kind/durability/size/ctor matrix
system_event/mod.rs   156 lines   SystemEvent + WarningKind + SteeringSource + kind()
system_event/tests.rs 228 lines   wire-tag matrix + round-trip + rejection tests
```

Each production file ends with `#[cfg(test)] mod tests;`; each
tests file opens with a module doc stating what it covers and
which fixtures it owns. `events/mod.rs`'s `pub use` re-exports
are untouched — the `events::AgentEvent` / `events::SystemEvent`
paths every consumer names (the crate root, the loop's sink
helpers, delegation's trace forwarder, the examples, the server)
resolve exactly as before.

## Verification

- `cargo test -p synthia-agent --lib` **247/247** — baseline
  identical; the 33 extracted tests run at
  `events::event_enum::tests::*` /
  `events::system_event::tests::*` (65/65 in the events module
  overall, same count as pre-split).
- `make ci` **7/7 green** (fmt-check, clippy `-D warnings`,
  rustdoc `-D warnings`, MVP-deps, runtime-free,
  public-API-runtime, claim-language, clock).
- `cargo test -p synthia-server --lib` **403/403**;
  `synthia-delegation` **66/66**.
- `cargo run -p synthia --example assemble_from_zero` →
  `ASSEMBLE-FROM-ZERO: OK` (drives the event stream end to end).
- `cargo +nightly fmt --all` clean.

## Result

The `events` module now has the same one-concern-per-file shape
as every other module in the crate. Every production file in
`synthia-agent/src/` is now under 500 lines except
`re_act/loop_/drive.rs` (603 — R98's orchestrator, whose size is
its named phases and which is single-concern by design). The
inline test blocks over 200 lines that remained in the crate are
gone.
