# turn-id-mvp

## Summary

`crates/synthia-agent/src/turn.rs` provides a `TurnId(Uuid)` newtype for
cross-event turn correlation in observability. `LoopContext` carries
`current_turn_id: Option<TurnId>`. `stream_builder/builder.rs` reads the
typed field instead of the legacy `format!("turn-{}", iteration)`
helper.

## Why

The 4-party adversarial review on 2026-06-13 rejected the full Turn
model (13-field struct + 4 new events + ~400 lines) for lacking concrete
callers, but accepted the simplified派 MVP (~20 lines) as the
YAGNI floor for "cross-event turn correlation." The change was
originally FROZEN for 3 months (→ 2026-09-13), but the user explicitly
overrode the freeze on 2026-06-13: "不要搞什么冻结了，给我干就完事了."

## Impact

- Non-breaking: existing `iteration: usize` retained, hook context
  `AgentContext.turn_id: String` unchanged (no consumer ripple)
- Adds: 1 file `turn.rs` (24 lines), 1 test file `tests/turn_id_test.rs`
- Removes: 1 file `turn_id.rs` (replaced by `turn.rs`)
- Modifies: 3 files (`lib.rs`, `loop_context.rs`, `stream_builder/builder.rs`)
- Zero new `AgentEvent` variants, zero persistence, zero state machine

## Validation

- `cargo check --workspace --all-targets`: 0 errors
- `cargo test --workspace --lib`: all pass
- `cargo +nightly fmt --all`: no changes
- `cargo clippy --all-targets --all-features --tests --all`: 0 new warnings
- `openspec validate turn-id-mvp --strict`: valid
- `openspec spec validate turn-id-label --strict`: valid
- `scripts/check_reexports.sh`: 5/5 passed

## Spec

`openspec/specs/turn-id-label/spec.md` (cumulative format)

## Archive

`openspec/changes/archive/2026-06-14-turn-id-mvp/`
