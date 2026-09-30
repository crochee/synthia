# Optimization report R93 — 2026-09-16

## What the audit found

R92 closed the harness crate. The workspace scan's next
worst production function: `run_controller_loop`
(`synthia-server/src/session/controller.rs`) at **144/20**
— 6× over the threshold, 373 lines, the session
controller's op loop. Shape: a `tokio::select!` with four
arms; the op arm held a seven-variant `SessionOp` match with
full inline bodies (the Prompt arm alone carried a
15-line comment); the run-completion and idle arms carried
inline guard futures plus their bodies.

## What landed

Behaviour byte-identical — the 403 synthia-server tests
(including the lifecycle, cancel, rerun, prompt_multi, and
run_config suites that pin exactly these paths) pass
unchanged.

### Op dispatch

- `handle_received_op` owns the seven-variant match and
  returns `bool` (`Shutdown` → true → the loop breaks with
  `Interrupted`).
- One handler per variant: `handle_prompt_op`,
  `handle_prompt_multi_op`, `handle_rerun_op`,
  `handle_feedback_op`, `handle_steer_op`,
  `handle_cancel_op`, `handle_shutdown_op` — each carrying
  its own doc comment (the "why persist at run completion"
  rationale moved with the code it explains).
- Shared helpers replace repeated inline bodies:
  - `push_text_input` — `Prompt` and `Steer` differ only in
    their log line.
  - `cancel_inflight_run` — `Rerun` and `Cancel` fire the
    same 5-line token dance.
  - `start_run_if_idle` — the uniform post-op starter
    (`if run_handle.is_none() && let Some(h) =
    maybe_start_run()`), used by four arms.

### Select arm bodies

- `on_run_completed` — panic surfacing plus the restart
  gate (with the comment explaining why the gate must be
  `maybe_start_run` itself, not a text-queue peek).
- `handle_idle_timeout` + `join_run` — the idle and else
  exit paths.

### Guard futures

`wait_for_run` / `wait_idle_timeout` hoisted out of inline
`async` blocks — also converting `Some(ref mut h)` to match
ergonomics (`match &mut run_handle { Some(h) => h.await }`).

### Cancel decomposition

`handle_cancel_op` (itself 26/20 after the first pass)
splits further: `cancel_inflight_run`, `drain_pending_inputs`,
`publish_cancelled_snapshot`.

## Result

```
$ cargo clippy -p synthia-server -W cognitive_complexity | grep controller
(28/20) controller.rs:812   (pre-existing, untouched)
(29/20) controller.rs:858   (pre-existing, untouched)
(33/20) controller.rs:1529  (pre-existing, untouched)
```

`run_controller_loop` (144) and `handle_cancel_op` (26):
**gone**. The loop now reads as its skeleton — select, three
one-line arm bodies, exit bookkeeping — with every concern
in a named helper below it.

## Verification

- `make ci` **7/7 green**.
- `make test-unit` **2446/2446**; `synthia-server` 403
  unchanged.
- `cargo +nightly fmt --all` clean.

## Deferred

R94 candidate: the `ControllerInner` trio — 812 (28/20,
`take_parked_prompt`-adjacent), 858 (29/20,
`maybe_start_run`), 1529 (33/20, `persist_and_broadcast`) —
plus `synthia-provider`'s streaming processor (39/20) and
the scattered 21-26s (`boot.rs` ×3, `log_surface.rs`,
`chat.rs`, `skill/discovery.rs`, `idle_watchdog.rs`,
`steering/hook.rs`).