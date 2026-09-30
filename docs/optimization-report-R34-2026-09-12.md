# R34 Absorption Results — 2026-09-12

Plan: [`optimization-report-R34-2026-09-12-plan.md`](optimization-report-R34-2026-09-12-plan.md).
Predecessor: [`optimization-report-R33-2026-09-12.md`](optimization-report-R33-2026-09-12.md).

R34 attacked the failure mode R33 kept finding: **a complete mechanism
whose last centimetre was never connected**. This round's audit found
the most consequential version of it yet — a durable checkpoint that
was one call from working — plus the missing "one dependency, from
zero" entry point.

## What the audit found

| Finding | Evidence |
|---|---|
| **The durable compaction checkpoint was one call short.** The manager fires record + lifecycle emitters, `synthia-session` had a bridge, `fold_surface` already validates and applies `SurfaceOp::Replace` — and **no production path installed either emitter**, so the log never recorded a compaction and a resume replayed the pre-compaction history | `context_manager_for_compaction` built `SummarizingContextManager::new(summarise).with_settings(settings)` with no `.with_compaction_emitter(..)`; greps for the bridge outside tests returned nothing |
| The tool surface was expressible only in code: exposure was set at registration, the tier cap and named groups had **no consumer**, and the server had no `[tools]` config | `adaptive.rs`/`grouped.rs` with no production caller; no tool section in `synthia-server/src/config/**` |
| A consumer needed 5–10 crates with the right features to assemble an agent — no facade, no prelude, despite "use it as a library, from zero" being this repo's stated purpose | no `pub mod prelude` anywhere; no `synthia` facade in `[workspace.members]` |
| `best_of` had one fixed rule; a host could not judge candidates | `BestOfStep` in `crates/synthia-workflow/src/spec.rs`, no selection hook on `WorkflowHost` |

## What landed

### A. The `synthia` facade — one dependency, one prelude, one tutorial

New workspace member `crates/synthia` (19 members now), **re-exports
and documentation only** — no logic, so it cannot drift behaviourally.
Every piece is a module (`synthia::agent`, `synthia::tool`,
`synthia::provider`, …) and `synthia::prelude` is a curated 27-name
subset with each collision decided and documented:

- `Result` → `synthia_core::Result<T, E = Error>` (the `synthia-tool`
  alias is reachable as `synthia::tool::Result`, not re-exported, since
  re-exporting both would shadow one for no gain);
- `Tool` → the **trait**; the `#[derive(Tool)]` macro keeps the name at
  `synthia::macros::Tool`;
- `Context` → `synthia_tool::Context` (the slice verified that the
  provider crate does **not** export a `Context` in this revision
  rather than repeating the plan's assumption);
- `SessionEndReason` (two genuinely different types, agent-run vs
  sink-close) is deliberately **excluded** and the exclusion documented.

Features mirror the pieces: the default set is what assembling a basic
agent needs (`core`, `provider`, `context`, `tool`, `session`,
`steering`, `agent`, `macros`); `skill`, `attachment`, `mcp`, `rag`,
`scheduler`, `eval`, `workflow`, `telemetry`, `sqlite`, `server`,
`test-support` are opt-in, so the facade never forces a heavy
dependency. Eleven `compile_fail` doc tests plus a feature-gating
integration test pin the gating; the crate docs are the tutorial
(provider → tools → steering → context manager → sink → cancel token →
agent), and `examples/assemble_from_zero.rs` assembles an agent through
the prelude only.

### B. `ToolSurfacePolicy` — the R33 seam, reachable from a deployment

- `ToolRegistry::{set_exposure, set_hidden, exposure}` mutate an entry
  after registration and report whether the name existed; a successful
  mutation refreshes the registry `version`, so the server's definition
  cache invalidates. The slice also found that cache was a **global
  `static`** shared by every `AppState` and moved it per-`AppState`.
- `ToolSurfacePolicy { max_visible, groups, active_groups }` with
  `apply(&registry)` (strict: unknown tool, empty group name, duplicate
  or twice-claimed tool → `SurfacePolicyError`, nothing written) and a
  deliberately lenient `visible_tool_names(&descriptors)` for the hot
  path. A member of an **inactive** group is written `Hidden`
  (advertising only — dispatch is untouched); a groupless tool stays
  `Direct`.
- Agent wiring: `ReActAgent::with_tool_surface`,
  `AgentBuilder::tool_surface`, `AgentRunConfig.tool_surface`, and the
  loop's `tool_definitions` applies the filter on top of the R33
  projection. With no policy the definitions are **byte-identical**
  (the R33 regression test was extended, not duplicated).
- Deployment: a `[tools]` server config section (`deferred`, `hidden`,
  `groups`, `active_groups`, `max_visible`) applied at boot, where an
  unknown tool name is a **warning, not a startup failure** — an MCP
  server that failed to register must not take the deployment down —
  with `AppliedToolSurface::skipped` recording what was ignored and a
  boot-wiring integration test proving config → operator listing →
  projected definitions.

### C. Host-side candidate selection for `best_of`

`WorkflowHost` gains
`select_candidate(&SelectionRequest) -> Result<Option<usize>, WorkflowError>`
with a **default body returning `None`** (use the built-in
first-passing rule), so no host breaks and the document format is
unchanged. `SelectionRequest` carries the step id, agent, each
candidate's prompt, outcome (`CandidateOutcome`) and gate verdict
(`GateVerdict`) — enough for a rubric or an LLM judge.

The decision is **recorded, not re-derived**: `JournalEntry.winner` and
`CallRun.winner` persist the choice, so a replay never asks the host
again. A host that names a candidate which did not succeed is a typed
`WorkflowError::Selection { step_id, selected, reason }` that stops the
run — chosen over a silent fallback because a wrong judge answer would
otherwise hide behind a plausible winner, and every candidate has
already settled and been journaled, so stopping wastes nothing. The
`Arc<T>` forwarder was extended too: without it an `Arc<MyHost>` would
have silently taken the default and ignored the override.

### D. Durable compaction checkpoints, end to end

The last call is made, and the mechanism in between was replaced rather
than patched:

- **New `SurfaceLedger`** resolves provenance through the log, not
  through the manager's in-memory indices — those cannot address the
  surface (the loop prepends a never-logged system prompt, and one
  assistant turn can span several rows). Each record cites tool-call
  ids, which the ledger maps to the seqs of the rows the run actually
  appended; `resolve` requires the count to match, every key to resolve
  uniquely, and the rows to be contiguous.
- **`CompactionCheckpoint`** installs both manager callbacks on the
  library path (`AgentBuilder`, when compaction settings and a typed
  sink are both present) and the server path
  (`compose_context_manager_with_emitters`). The lifecycle triple is
  written immediately (so a crash between start and end stays
  detectable by `orphaned_compactions`); the `Replace` checkpoint is
  written **only when its provenance is provable**. A record emitted
  before the run's rows land parks and is retried by
  `resolve_pending`; an unprovable record is dropped with a warn and
  the surface stays foldable.
- **Clean cutover, not a second path**: `compaction_emitter_bridge`,
  `CompactionRecordView::to_typed_event` and
  `TypedEventSink::record_compaction` were **removed**. All three wrote
  a `Replace` with the manager's indices and empty
  `source_event_seqs` — exactly the wrong-provenance write the design
  forbids, and nothing outside their own tests used them.
- **The production resume projection honours the ops**:
  `synthia_session::log_surface::{fold_log_surface, try_fold_log_surface,
  surface_events_from_log}` fold a raw log, and
  `synthia-context`'s typed/session memory paths use them, so a resumed
  session rebuilds the compacted surface instead of replaying the
  pre-compaction span — the property that motivates the whole slice.

Observed in `examples/compaction_checkpoint.rs`: a record citing the
tool result at seq 3 writes `surface_op={"start":2,"end":3,
"source_event_seqs":[3]}` (durable span 2..3 derived from the log, while
the manager reported index 3), the folded surface becomes
`[1, 2, 6]`, the tool result is `before=true after=false`, strict and
lenient folds agree, and an unprovable record writes **no** compaction
row while the lifecycle rows still land 1/1/1 and the log still folds.

## Verification (final state)

| Gate | Result |
|---|---|
| `cargo +nightly fmt --all --check` | exit 0 |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | exit 0 (0 warnings) |
| Per-crate `cargo test -p <crate>` (credential-free env) | **2520 passed / 0 failed** across **19** crates, every crate exit 0 |
| `cargo test -p synthia-context --features sqlite` | 103 passed / 0 failed |
| `cargo check -p synthia` / `--all-features` | exit 0 / exit 0 (the facade's gating is real) |
| Example ladder | **28/28 exit 0** (new: `ASSEMBLE-FROM-ZERO`, `TOOL-SURFACE-POLICY`, `COMPACTION-CHECKPOINT`) |
| `docs/examples/external-consumer` | `CONSUMER-PROOF: OK` |

Per-crate totals, with the R33 → R34 delta:

| Crate | R34 | Δ |
|---|---|---|
| core | 94 | — |
| telemetry | 36 | — |
| provider | 731 | — |
| context | 95 (103 with `sqlite`) | +2 |
| tool | 307 | +6 (mutation API, policy) |
| skill | 56 | — |
| session | 130 | +5 (ledger, checkpoint, log fold) |
| steering | 74 | — |
| agent | 306 | +4 (policy plumbing, checkpoint callbacks) |
| server | 428 | +9 (tools config, boot application, threading) |
| attachment / mcp / rag / scheduler / macros / eval / test-support | 15 / 52 / 38 / 14 / 31 / 36 / 18 | — |
| workflow | 45 | +11 (host selection) |
| **synthia** (facade, new) | 14 | +14 |

R33 was 2570 (2469 + 101 with `sqlite`); R34 is 2623 (2520 + 103) —
**+53 tests**, and the workspace is now 19 members.

## Deferred to R35 (recorded, not dropped)

`assistant/chunk` live-resume (pi delta frames); `SessionEventMap`
extensibility; FsAdapter error boundary; `shutdownChildSession`
lifecycle refinements; `DeferredHandle` / cross-gateway
`OperationRequest` polling; partial-JSON *display* for mid-stream UIs;
MCTS-style tree search (as opposed to R33's flat selection and R34's
host-side judgement); provider auth flows (PKCE / device-code /
credential store).
