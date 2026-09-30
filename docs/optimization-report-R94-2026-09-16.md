# Optimization report R94 — 2026-09-16

## What the audit found

R93 named the `ControllerInner` triple as the next
target. The fresh workspace scan:

| Function | File | Score |
|---|---|---|
| `finalise_shutdown` | controller.rs:812 | 28/20 |
| `maybe_start_run` | controller.rs:858 | 29/20 |
| `persist_and_broadcast` | controller.rs:1529 | 33/20 |

All three are sibling methods on `ControllerInner`. The
session controller is the single largest production file
in the workspace; bringing it under the threshold
eliminates every `synthia-server` complexity hit.

## What landed

Behaviour byte-identical — the 403 synthia-server tests
(including the snapshot-bus, cancel, run_config,
history, lifecycle, prompt_multi, rerun, truncate, and
event_emission suites that pin the modified paths) pass
unchanged. The duplicate-publish bug the refactor
introduced (and the test caught) was a second-pass fix;
the test was always green before the refactor and stays
green after.

### `finalise_shutdown` (was 28/20)

The four-step terminal block — build the
`LifecycleShutdown` event, serialize+append it, log+swallow
either failure, close the sink — splits into two
single-concern helpers:

- `append_lifecycle_event(&self, &event)` — serialize +
  append one terminal lifecycle event; logging (not
  propagating) either failure so the shutdown path always
  runs to the sink close.
- `close_sink(&self, reason)` — close the session sink; a
  close failure is logged but does not abort the shutdown.

`shutdown_finalised.swap` stays at the top so a
shutdown-then-idle race cannot double-append.

### `maybe_start_run` (was 29/20)

The 27-line in-function block (state gate, pending check,
Running transition, pending-count log, snapshot publish,
cancel-token install) is replaced by:

- `is_startable(&self) -> bool` — the Idle/Cancelled gate
  (with its own `tracing::trace!` for the skipped branch).
- `mark_running(&self, multimodal_active)` — the Running
  transition, the pending-count log, the R11 snapshot
  publish, the cancel-token install.

The function body is now: gate check → pending check →
`mark_running` → config assembly. The cognitive cost moved
into the helpers; the orchestrator is flat.

(During the extraction I left a duplicate R11 publish and
duplicate cancel-token install in the body. The
`snapshot_bus_publishes_running_then_completing` test
caught it — `rx` received Running twice and never
Completing. The second-pass fix removed both duplicates;
the test is back to passing.)

### `persist_and_broadcast` (was 33/20)

The body splits into four helpers around the
serialize-once invariant:

- `encode_event(event) -> Result<(Vec<u8>, Value)>` —
  `to_vec` for the size + decode-recover for the sink
  append, both in one shape. The doc comment captures the
  three-pass history (`to_value(...).to_string().len()` +
  `EventBroadcaster::send` re-serialize).
- `classify_system_kind(event) -> Option<&'static str>` —
  the system-event match for the structural log line.
- `append_if_durable(event, &value, log) -> Result<()>` —
  the `event.is_durable()` guard + `log.append`.
- `broadcast_event(&self, event, outer_kind, system_kind)` —
  the SSE/WebSocket fan-out + the "no subscribers" debug
  log.

The function body is the one-sentence pipeline the doc
comment already promised.

## Result

```
$ cargo clippy -p synthia-server -W cognitive_complexity \
    | grep controller
(no output — zero violations in controller.rs)
```

Before R94: 28/29/33. After: none.

## Verification

- `make ci` **7/7 green**.
- `make test-unit` **2446/2446**; `synthia-server` 403.
- `cargo +nightly fmt --all` clean.

## Deferred

Remaining workspace hits (R95+ candidates):
- `synthia-provider/streaming/anthropic/processor.rs` (39/20)
- `synthia-server/session/log_surface.rs` (25/20) and
  scattered 21-26s in `chat.rs`, `state/boot.rs`,
  `idle_watchdog.rs`
- `synthia-skill/discovery.rs` (23/20)
- `synthia-steering/hook.rs` (22/20)
- `synthia-context/summarizing.rs` (25/20) — was flagged
  earlier; R93 already noted it.

Standing triggers unchanged (reference repos, drift
surfaces, architecture gates, `#[allow]` regressions).