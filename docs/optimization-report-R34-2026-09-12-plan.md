# R34 Absorption Plan — 2026-09-12

Sequel to R33 (`608033a0`). Standing objective unchanged: absorb the
best designs from `~/workspace/traitclaw/`, `~/workspace/deepseek-harness/`,
`~/workspace/pi/` and `~/workspace/pi-subagents/` into a runtime-neutral,
Lego-composable Rust agent framework that works as a library and in
production.

R34 attacks the class of gap R33 found twice: **a complete mechanism
whose last centimetre was never connected**. This round was again shaped
by reading the tree, and the audit produced one finding worth stating
plainly before the slices.

## What the audit found

| Finding | Evidence |
|---|---|
| The tool surface is expressible only in code: exposure is set at registration, the tier cap (`AdaptiveRegistry`) and named groups (`GroupedRegistry`) still have **no consumer**, and there is no `[tools]` server config | `crates/synthia-tool/src/adaptive.rs`, `grouped.rs` (no production caller); `crates/synthia-server/src/config/**` has no tool section |
| **The durable compaction checkpoint is one call short of working.** `SummarizingContextManager` fires lifecycle + record emitters, `synthia_session` has the bridge (`CompactionRecordView::to_typed_event`, `compaction_emitter_bridge`) and `fold_surface` already validates and applies `SurfaceOp::Replace` provenance — but **no production path installs either emitter**, so the log never records a compaction and a resume replays the *pre-compaction* history | `compose_context_manager` → `synthia_agent::context_manager_for_compaction` builds `SummarizingContextManager::new(summarise).with_settings(settings)` with no `.with_compaction_emitter(..)`; greps for `with_compaction_emitter`/`compaction_emitter_bridge` outside tests return nothing |
| A consumer must depend on 5–10 crates with the right features to assemble an agent; there is no facade and no `prelude`, which is exactly the "use it as a library, from zero" story this repo is supposed to teach | no `pub mod prelude` anywhere; no `synthia` facade crate in `[workspace.members]` |
| `best_of` has one fixed rule (first passing gate); a host has no way to judge candidates better | `crates/synthia-workflow/src/spec.rs` `BestOfStep`; no selection hook on `WorkflowHost` |

## Slices

### A. `synthia` facade crate (`crates/synthia`, new member)

One dependency, one `prelude`, one tutorial. The crate contains **only
re-exports and documentation** — no logic, so it cannot drift
behaviourally: every module is one underlying crate (`synthia::agent`,
`synthia::tool`, …) and the prelude is a curated subset with each name
collision resolved deliberately and documented (`Context` exists in both
`synthia-tool` and `synthia-provider`).

Feature flags mirror the pieces: the default set is what assembling a
basic agent needs (`core`, `provider`, `context`, `tool`, `session`,
`steering`, `agent`, `macros`); `skill`, `attachment`, `mcp`, `rag`,
`scheduler`, `eval`, `workflow`, `telemetry`, `sqlite`, `server` and
`test-support` are opt-in, so the facade never forces a heavy
dependency on a consumer who does not want it.

Its crate docs are the tutorial: the pieces in the order a consumer
meets them (provider → tools → steering → context manager → sink →
cancel token → agent), ending with the one-expression `AgentBuilder`
form, plus an offline example that assembles an agent through the
prelude only.

### B. `ToolSurfacePolicy` — the R33 seam, reachable from a deployment

The R33 projection is the seam; this slice makes it configurable.

- registry mutation with a fail-visible result:
  `set_exposure`, `set_hidden`, `exposure` (an unknown name returns
  `false`/`None`, never a silent no-op), with the registry `version`
  refreshed so the server's definition cache invalidates — an invariant
  that must be made true rather than hoped for;
- `ToolSurfacePolicy { max_visible, groups, active_groups }` with
  `apply(&registry)`, validating names like `GroupError` does today (an
  unknown tool or a tool claimed by two groups is a typed error and
  changes nothing);
- agent wiring (`ReActAgent::with_tool_surface`,
  `AgentBuilder::tool_surface`) where the loop's `tool_definitions`
  applies the visible-name filter — **with no policy the definitions
  stay byte-identical to R33**, pinned by extending the existing
  regression test rather than writing a parallel one;
- a `[tools]` server config section (`deferred`, `hidden`, `groups`,
  `active_groups`, `max_visible`) applied at boot, where an unknown tool
  name is a **warning, not a startup failure** — an MCP server that
  failed to register must not take the deployment down — with a boot
  test proving config → operator listing and config → projected
  definitions.

Divergence from any single reference: this is synthia's own seam
(`ToolExposure` + one projection). pi's `splitDeferredTools` splits the
advertised list from the transcript; synthia makes the *policy* data and
keeps the transcript as the promotion signal.

### C. `best_of` host-side selection

R33's rule (first passing gate) stays the default; the **host seam**
gains the ability to judge. `WorkflowHost` gets an async
`select_candidate(&SelectionRequest) -> Result<Option<usize>, _>` with a
default implementation returning `None` (use the built-in rule), so no
existing host breaks, the document format does not change, and replay
stays honest: a replayed run must not consult the host again.

`SelectionRequest` carries the step id, the agent, and per candidate its
prompt, outcome and gate verdict — enough for a rubric or an LLM judge,
without leaking runtime internals.

### D. Durable compaction checkpoints, end to end

The mechanism exists; the last call does not. This slice installs it and
proves the property that matters:

- the shared factory (`synthia_agent::context_manager_for_compaction`)
  and the server's composition site install the session bridge, so a
  compaction writes `compaction_start` / `compaction_summary` /
  `compaction_end` plus a `SessionEvent::Compaction` whose `SurfaceOp::Replace`
  cites the shadowed events' **seqs** (the bridge's view carries message
  indices; mapping them to the surface's seqs is this slice's real design
  work and the reason the checkpoint must be *foldable*, not merely
  written);
- a resume/replay test proves `fold_surface(log)` yields the compacted
  surface and that the pre-compaction span is not replayed — the token
  saving is the whole point;
- a crash between start and end stays detectable
  (`orphaned_compactions`), which is why the lifecycle pair ships with the
  checkpoint rather than after it.

Sequenced after wave 1 because it edits `synthia-agent/src/agent/builder.rs`
(the shared factory), which slice B also touches.

## Verification (final state)

- `cargo +nightly fmt --all --check` — exit 0.
- `cargo clippy --all-targets --all-features --tests --all -- -D warnings` — 0 warnings.
- Per-crate `cargo test -p <crate>` for all **19** crates (the facade is new) in a credential-free environment, plus `cargo test -p synthia-context --features sqlite`.
- `cargo check -p synthia` and `cargo check -p synthia --all-features`.
- Every example runs offline (the 25 from R33 plus the new ones).
- `docs/examples/external-consumer` → `CONSUMER-PROOF: OK`.
- README rows + member inventory (19), `docs/examples/README.md`, CHANGELOG, `docs/optimization-report-R34-2026-09-12.md`.

## Deferred to R35 (recorded, not dropped)

`assistant/chunk` live-resume (pi delta frames); `SessionEventMap`
extensibility; FsAdapter error boundary; `shutdownChildSession`
lifecycle refinements; `DeferredHandle` / cross-gateway
`OperationRequest` polling; partial-JSON display for mid-stream UIs;
MCTS-style tree search (as opposed to R33's flat selection); provider
auth flows (PKCE / device-code / credential store).
