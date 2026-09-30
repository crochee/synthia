# R33 Absorption Plan — 2026-09-12

Sequel to R32 (`2b492426`). Standing objective unchanged: absorb the
best designs from `~/workspace/traitclaw/`, `~/workspace/deepseek-harness/`,
`~/workspace/pi/` and `~/workspace/pi-subagents/` into a runtime-neutral,
Lego-composable Rust agent framework that works as a library and in
production.

R33 was shaped by two reconnaissance passes that read the actual seams
rather than the plan documents. Both found the same kind of gap: a
mechanism that exists as vocabulary but that **nothing consumes**.

## What the reconnaissance found

| Finding | Evidence |
|---|---|
| `ToolExposure::{Direct, Deferred, Hidden}` is declared but read by no code path | only `default()` is tested; no field stores it (`crates/synthia-tool/src/registry.rs:49`) |
| The model-facing tool list is built **twice**, both from `list(None)` + `Tool::parameters()`, so `snapshot`/`AdaptiveRegistry`/`GroupedRegistry` have no production consumer | `crates/synthia-agent/src/agent/re_act.rs:1615`, `crates/synthia-server/src/routes/tool.rs:80` |
| Tool-call arguments are finalized by **two parsers with different fallbacks** — `Value::String(raw)` vs `Value::Null` — so identical input yields different `ToolUse.input` depending on the path | `crates/synthia-provider/src/streaming/tool_args.rs:41` vs `crates/synthia-provider/src/assembler.rs:389` |
| No JSON repair / tolerant-parse helper exists anywhere; a truncated or malformed tool-call argument is silently degraded | greps for `repair`, `parse_streaming`, `partial_json` over `crates/` → none |
| Retry classification sees only the HTTP status and raw body **text**; a structured body (`{"error":{"type":"insufficient_quota"}}`) is matched by string markers | `crates/synthia-provider/src/retry.rs:232-264`; adapters pass `response.text()` |

## Slices

### A. Tool surface projection + deferred exposure (`synthia-tool`, `synthia-agent`, `synthia-server`)

pi `packages/ai/src/utils/deferred-tools.ts` (`splitDeferredTools`) is
the reference: the transcript decides how much of a tool the model is
shown. synthia has the vocabulary (`ToolExposure`) and a schema-less
projection nobody calls, so the slice makes both real:

- exposure becomes entry data (`ToolEntry::with_exposure`), carried onto
  `ToolDescriptor` with a serde default so old wire data keeps working;
- one pure projection turns descriptors + an optional visible-name
  filter + the set of already-called names into `Vec<ToolDefinition>`:
  `Direct` → full schema, `Deferred` → name + description with a
  permissive schema until first call, then full schema, `Hidden` →
  absent;
- promotion is derived from the **transcript** (`called_tool_names`), not
  from mutable registry state, so it is replay-stable and runtime-neutral;
- both call sites (`ReActAgent::tool_definitions`, `collect_tool_defs`)
  use the projection, so `AdaptiveRegistry`'s tier cap and
  `GroupedRegistry`'s groups finally have a consumer;
- a registry with no `Deferred`/`Hidden` tools must produce
  byte-identical definitions to today (regression test).

Divergence from pi: pi moves deferred tools out of the advertised list
entirely and relies on the transcript mentioning their names; synthia
keeps them callable with a permissive schema, because a provider that
rejects an unadvertised tool name would turn a prompt-economy choice
into a hard failure.

### B. Provider wire robustness (`synthia-provider`, maybe `synthia-core`)

Two designs from pi:

- `packages/ai/src/utils/json-parse.ts` (`repairJson`,
  `parseStreamingJson`) becomes `synthia_provider::json_repair`:
  `repair_json` escapes raw control characters inside string literals and
  doubles backslashes before invalid escapes, leaving valid JSON
  byte-identical; `parse_tool_input_reported` reports whether the value
  came from a strict parse, a repaired parse, or the raw fallback. Every
  finalization path (both processors, `BlockAssembler`) routes through
  the one shared function, which **deletes the divergent duplicate** and
  makes the fallback a documented decision instead of an accident.
- `packages/ai/src/utils/error-body.ts` (`normalizeProviderError`)
  becomes `synthia_provider::error_body`: a typed
  `ProviderErrorBody { status, kind, code, message, body_excerpt }` parsed
  from the real provider shapes, which classification consults so that
  **quota/billing, rate limit, context overflow and auth/permission** are
  distinguishable from a JSON body alone. A text-only body must classify
  exactly as it does today.

Divergences: pi's normalizer exists to paper over four heterogeneous JS
SDKs; synthia owns its adapters, so the Rust version normalises the two
shapes it actually produces and keeps a bounded excerpt for everything
else. pi's repair is string-level with no report; synthia reports the
quality so a production log shows salvaged arguments.

### C. Workflow `best_of` selection (`synthia-workflow`)

traitclaw `crates/traitclaw-strategies/src/mcts/strategy.rs` is the
reference: parallel branches with a scoring function and a recorded
winner. Adapted to synthia's declarative runtime, a new step kind beside
`agent` / `fan_out` / `pipeline`:

- `best_of` plans one call per item (each its own journal position, so
  prefix replay keeps working) under the existing caps and concurrency
  bound;
- the first candidate whose gate passes — or, with no gate, the first
  success — wins; its text becomes the step's chained output;
- losers are recorded with a status that never makes a passing run
  `failed`, and a candidate that fails outright does not abort the others;
- if nobody wins the step fails with every candidate's failure text.

Divergence from traitclaw: no VM, no scoring closure — the selector is
the gate the workflow already knows how to run, so the pattern stays
declarative, serde-round-trippable and replayable.

## Verification (final state)

- `cargo +nightly fmt --all --check` — exit 0.
- `cargo clippy --all-targets --all-features --tests --all -- -D warnings` — 0 warnings.
- Per-crate `cargo test -p <crate>` for all 18 crates **in a credential-free environment** (the R32 standard), plus `cargo test -p synthia-context --features sqlite`.
- Every example runs offline: the 22 from R32 plus the new ones.
- `docs/examples/external-consumer` → `CONSUMER-PROOF: OK`.
- README primitive rows + `docs/examples/README.md` index + CHANGELOG entry + `docs/optimization-report-R33-2026-09-12.md`.

## Deferred to R34 (recorded, not dropped)

Compaction log-only events + `SurfaceOp.replace` checkpoint;
`assistant/chunk` live-resume (pi delta frames); `SessionEventMap`
extensibility; FsAdapter error boundary; `shutdownChildSession`
lifecycle refinements; `DeferredHandle` / cross-gateway
`OperationRequest` polling; wire `AdaptiveRegistry`/`GroupedRegistry`
into a deployment-level config (the projection is now the seam for it);
MCTS-style scoring closure on `best_of`; provider auth flows (PKCE /
device-code / credential store).
