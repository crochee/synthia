# R9 follow-up: legacy `synthia-session/src/store/` collapse

## Status: **DEFERRED** (R3 partial follow-up)

## Decision

R9 was originally planned as "port 7 internal callers + delete legacy
`store/`". During Round 9 pre-flight investigation, the actual caller
surface turned out to be **14 files**, not 7. Porting all 14 callers
to the v2 types within a single 30-minute dispatch budget exceeded
the project's "no auto-commit + STOP after each round" rule and
re-introduced the same plan-vs-reality risk that timed out R3.

## What was done in R3 (the actual collapse work)

`crates/synthia-session/src/store/mod.rs` (commit `facd3a9`) was
already collapsed into a **thin re-export shim**:

- It re-exports the new v2 types from `synthia_session_v2::*`
  (`AgentPart`, `Message`, `Part`, `SessionEntry`, `SessionTree`,
  `ToolPart`, `ToolState`, etc.) under `#[allow(deprecated)]`.
- It still serves the legacy façade types (`Store`, `SessionMetadata`,
  `EventStore`, `PersistedEvent`, `SessionInputQueue`, `CheckpointData`)
  for the 14 external callers that haven't been ported yet.
- It still owns the on-disk layout (`metadata.json` + `messages.jsonl` +
  `events.jsonl` + `checkpoint_{step}.json`) because the legacy callers
  read/write those files directly.

## What remains for a future R9

To complete the collapse, the 14 callers must be ported to v2 types
ONE AT A TIME (each in its own commit, each with TDD coverage):

1. `crates/synthia-session/src/service.rs`
2. `crates/synthia-cli/src/repl_core/repl/agent_message.rs`
3. `crates/synthia-cli/src/repl_core/repl/execute/session.rs`
4. `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs`
5. `crates/synthia-agent/src/stream_builder/builder/iteration/init.rs`
6. `crates/synthia-agent/src/events/persisted.rs`
7. `crates/synthia-agent/src/replay.rs`
8. `crates/synthia-agent/src/config/agent_config/run_config.rs`
9. `crates/synthia-server/tests/subagent_event_streaming.rs`
10. `crates/synthia-server/src/state/agent_factory.rs`
11. `crates/synthia-server/src/middleware/auth/user_id.rs`
12. `crates/synthia-server/src/middleware/auth/tests.rs`
13. `crates/synthia-server/src/session/controller.rs`
14. `crates/synthia-server/src/routes/v2/events.rs`

Once ALL 14 callers use v2 types exclusively, the `store/` directory
can be deleted in a single follow-up commit, leaving only:

```
crates/synthia-session/src/
├── lib.rs                          # re-exports from v2
├── service.rs                      # (port to v2)
├── state_machine/                  # (kept as-is)
└── (no more store/)
```

## Why we did NOT delete `store/` now

- Deleting `store/` today would break `cargo build --workspace`.
- 14 callers is **2× the planned 7**; the porting work is non-trivial.
- Each caller migration requires its own TDD red-green cycle.
- The project's `extension-points-phase-2/plan.md:273` rule says
  "no auto-commit — each round ends with '等用户明确指示'". A
  14-caller port + deletion in one round violates that rule.

## Acceptance

R9 deferred status was accepted as part of the v3 architecture
rollout. The legacy `store/` shim is acceptable long-term because
v2 types are already accessible via the same `synthia_session::*`
import paths (re-exported by `store/mod.rs:274+`).

Future cleanup should follow the 14-callers-one-at-a-time plan
above. Each port is its own round + its own STOP checkpoint.