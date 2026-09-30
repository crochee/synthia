# Optimization report R107 — 2026-09-18

## What the audit found

After R106 the largest remaining layout debt was production code,
not tests: `server/session/controller.rs` stood at 2 328 lines —
`SessionOp`/`SessionState` wire types, the `RunStreamFactory`
seam, `RunDependencies`, the `SessionController` handle, the
`ControllerInner` state with an 858-line impl (including a
533-line `maybe_start_run` run-task method), the dispatch loop
with eleven op handlers, `RunLog`, and three prompt-shaping
helpers, all in one file.

## What landed

`controller.rs` → `controller/` (one concern per file; the
pre-existing `tests/` subtree is untouched and its
`super::super::X` imports were retargeted to true sources):

| File | Lines | Concern |
|---|---|---|
| `mod.rs` | 218 | `SessionController` handle + re-exports (`SessionOp` / `SessionState` / `RunDependencies` / `ground_with_retrieval` / `AgentRunStreamFactory` / `RunStreamFactory` / `DEFAULT_IDLE_TIMEOUT` — every `crate::session::controller::X` path unchanged) |
| `ops.rs` | 70 | `SessionOp`, `SessionState` |
| `run_stream.rs` | 168 | the `RunStreamFactory` trait + production impl |
| `deps.rs` | 322 | `RunDependencies`, `ground_with_retrieval` |
| `inner.rs` | 304 | shared state, shutdown trio, state gates, run-config assembly |
| `run_task.rs` | 557 | `maybe_start_run` — the spawned run task, as an impl continuation |
| `persist.rs` | 112 | the event sink quintet (append/broadcast/classify) |
| `dispatch.rs` | 535 | the controller loop + op handlers + idle lifecycle |
| `run_log.rs` | 164 | `RunLog` + prompt-text shaping |

Cross-module seams are `pub(super)` only; nothing new is visible
outside `session::controller`. Doc links that crossed the new
module boundaries were qualified (`synthia_agent::AgentRunConfig`,
`super::SessionController`) so `doc-check` stays at zero.

## Verification

- `synthia-server` **403/403**, test-name list byte-identical to
  the pre-split baseline.
- `make ci` **7/7** (including `doc-check` and the claim-language
  gate); workspace clippy `--all-targets --all-features --tests
  --all` **0 warnings**; `cargo +nightly fmt --all --check`
  clean.

## Result

The workspace's largest production file is now
`tool/registry.rs` (1 369) followed by `server/routes/chat.rs`
(1 180), `context/memory/file.rs` (1 103 production),
`server/state/app_state.rs` (1 071), `delegation/pool.rs`
(1 036) — the next layout rounds. Inside `synthia-server` no
file exceeds 557 lines.
