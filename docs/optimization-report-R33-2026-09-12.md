# R33 Absorption Results — 2026-09-12

Plan: [`optimization-report-R33-2026-09-12-plan.md`](optimization-report-R33-2026-09-12-plan.md).
Predecessor: [`optimization-report-R32-2026-09-12.md`](optimization-report-R32-2026-09-12.md).

R33 was shaped by reconnaissance rather than by the deferral list.
Two read-only passes over the actual seams found the same class of gap
three times: **a mechanism that exists as vocabulary which nothing
consumes**.

| Gap found (with evidence) | What R33 did |
|---|---|
| `ToolExposure::{Direct, Deferred, Hidden}` declared, read by no code path (`crates/synthia-tool/src/registry.rs:49`; only `default()` was tested) | exposure became entry data, and one projection now consumes it in both definition builders |
| the model-facing tool list built **twice** from `list(None)` + `parameters()`, so `snapshot`, `AdaptiveRegistry` and `GroupedRegistry` had **no production caller** (`re_act.rs:1615`, `routes/tool.rs:80`) | both call sites use the new projection; the visible-name filter is how the tier cap and groups plug in |
| two tool-argument parsers with **different fallbacks** (`Value::String` vs `Value::Null`) in `streaming/tool_args.rs:41` and `assembler.rs:389`; no repair helper anywhere in `crates/` | one shared finalization path with repair and a reported quality; the duplicate is deleted |
| classification saw only HTTP status + raw body **text** (`retry.rs:232-264`), so a structured `insufficient_quota` body was matched by string markers | error bodies are parsed into a typed signal and classified from it |

## What landed

### A. Tool surface projection + deferred exposure

`crates/synthia-tool/src/surface.rs` (new), exposure plumbing in
`registry.rs`, and both definition builders wired to it.

```rust
// synthia-tool
impl ToolEntry { pub fn with_exposure(self, ToolExposure) -> Self; pub fn exposure(&self) -> ToolExposure; }
impl ToolRegistry { pub fn descriptors(&self) -> Vec<ToolDescriptor>; }   // sorted by name, no visibility filtering
pub fn project_tool_definitions(
    descriptors: &[ToolDescriptor],
    called: &HashSet<String>,
    visible: Option<&HashSet<String>>,
) -> Vec<ToolDefinition>;
pub fn called_tool_names(messages: &[Message]) -> HashSet<String>;
```

- `Direct` → full `Tool::parameters()` schema; `Deferred` (not yet
  called) → name + description verbatim with
  `{"type":"object","additionalProperties":true}`; `Deferred` (called) →
  full schema; `Hidden` / `is_hidden` → absent.
- Promotion is computed from the **transcript**
  (`called_tool_names`, which reads assistant tool-use blocks and
  tool-result names), so it is deterministic under replay and needs no
  mutable registry state. That is the divergence from pi's
  `splitDeferredTools`, which reads the same transcript but drops
  deferred tools from the advertised list entirely; synthia keeps them
  callable with a permissive schema, because a provider that rejects an
  unadvertised name would turn a prompt-economy choice into a hard
  failure.
- Two levels of hiding are now documented and pinned: `Hidden` exposure
  keeps a tool out of the model list while `run_stream` still dispatches
  it (`registry::tests::hidden_exposure_tool_stays_executable`);
  `is_hidden` removes it *and* refuses dispatch, as before.
- API note: `ReActLoop::tool_definitions` gained the transcript
  (`fn(&self, messages: &[Message])`, previously an `async fn(&self)`
  with no arguments) because promotion needs it; every in-repo caller
  was updated.
- Regression: with no `Deferred`/`Hidden` tools registered the produced
  definitions are unchanged, and the server's operator listing
  (`collect_tool_defs`, which only reads name + description) is
  transcript-free by construction — documented at that site.

### B. Wire hygiene — tool arguments and error bodies

`crates/synthia-provider/src/json_repair.rs` and `error_body.rs` (new),
plus `RetryClass::{ContextOverflow, Auth}` and
`classify_provider_error_body` in `retry.rs`.

```rust
pub enum ToolArgsQuality { Strict, Repaired, RawFallback }
pub fn repair_json(raw: &str) -> String;
pub fn parse_tool_input_reported(raw: &str) -> (Value, ToolArgsQuality);
pub fn parse_tool_input(raw: &str) -> Value;              // unchanged contract
pub fn parse_tool_input_logged(raw: &str, tool_name: &str) -> Value;  // WARN on salvage
pub struct ProviderErrorBody { status, kind, code, message, body_excerpt }
pub fn parse_provider_error_body(status: u16, raw: &str) -> ProviderErrorBody;
pub fn classify_provider_error_body(status: u16, raw: &str) -> RetryClass;
```

- `repair_json` escapes raw control characters inside string literals
  (`\n`, `\r`, `\t`, `\b`, `\f`, else `\uXXXX`) and doubles a backslash
  that precedes an invalid escape, leaving valid JSON byte-identical.
  Observed: `{"body":"line one<LF>line two"}` → `Repaired` with the
  intended value; `{"pattern":"C:\qemu\build"}` → `Repaired` with
  `C:\qemu\build` recovered.
- **Divergence from pi:** pi parses *incomplete* JSON (via
  `partial-json`) so a half-streamed argument can be displayed. synthia
  reports a truncated tail as `RawFallback` instead of completing it: a
  half-written argument must never be executed as if it were complete,
  and the R30 length-stop guard already owns that decision. The choice
  is visible in `examples/tool_arg_repair.rs`.
- The duplicate parser is gone: every finalization site (both stream
  processors and `BlockAssembler`) routes through the shared function,
  so identical input can no longer produce `Value::String` on one path
  and `Value::Null` on another. The non-`Strict` cases emit a
  `tracing::warn!` naming the tool and quality, with the raw text capped
  in the message.
- Classification now reads the body. Observed classes on realistic
  bodies: OpenAI `insufficient_quota` (429) → `Quota`; OpenAI
  `context_length_exceeded` (400) → `ContextOverflow`; Anthropic
  `overloaded_error` (529) → `Overloaded`; Anthropic `permission_error`
  (403) → `Auth`; a bodyless 429 → `RateLimit`. Both adapters' 429
  paths read the body they previously discarded.

### C. `best_of` — selection as a workflow step

`crates/synthia-workflow/src/spec.rs`, `plan.rs`, `runtime.rs`,
`result.rs`.

```rust
pub struct BestOfStep { id, agent, items: Vec<String>, gate: Option<GateRef>, max_items, concurrency }
Step::BestOf(BestOfStep)              // wire kind "best_of"
CallStatus::Superseded                // a candidate that ran and lost
impl CallRun { pub fn succeeded(&self) -> bool }
impl WorkflowRun { pub fn winner_of(&self, step_id: &str) -> Option<&CallRun> }
```

- One planned call per candidate (each its own journal position), under
  the existing caps and concurrency bound; the first candidate whose
  gate passes wins, or with no gate the first success. The winner's text
  is the step's chained output.
- Three orthogonal facts — did it run, did it succeed, did it win —
  cannot fit one status honestly, so losers get `Superseded` (a loser
  never drags a passing run down), a replayed loser keeps `Replayed`,
  control facts (skipped/aborted) keep their status, and an all-failed
  selection fails the step while preserving every failure text.
- `replay_prefix` learned one documented exception: a recorded failure
  inside a `best_of` step does not end the prefix when a sibling
  candidate has a matching success, so a resume does not re-run a
  selection that already decided.
- Divergence from traitclaw's `MctsStrategy`: no VM, no scoring closure.
  The selector is the gate the workflow already knows how to run, which
  keeps the step declarative, serde-round-trippable and replayable.

## Verification (final state)

| Gate | Result |
|---|---|
| `cargo +nightly fmt --all --check` | exit 0 |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | exit 0 (0 warnings) |
| Per-crate `cargo test -p <crate>` (credential-free env) | **2469 passed / 0 failed** across 18 crates, every crate exit 0 |
| `cargo test -p synthia-context --features sqlite` | 101 passed / 0 failed |
| Example ladder | **25/25 exit 0** (`DEFERRED-TOOLS: OK`, `TOOL-ARG-REPAIR: OK`, `WORKFLOW-BEST-OF: OK` + the 22 from R32) |
| `docs/examples/external-consumer` | `CONSUMER-PROOF: OK` |

Per-crate totals, with the R32 → R33 delta where it moved:

| Crate | R33 | Δ |
|---|---|---|
| core | 94 | — |
| telemetry | 36 | — |
| provider | 731 | +20 (repair, error bodies, 429 body test, salvage log) |
| context | 93 (101 with `sqlite`) | — |
| tool | 301 | +13 (surface + exposure) |
| skill | 56 | — |
| session | 125 | — |
| steering | 74 | — |
| agent | 302 | +1 (promotion after a call) |
| server | 419 | +1 (operator-listing projection) |
| attachment / mcp / rag / scheduler / macros / eval / test-support | 15 / 52 / 38 / 14 / 31 / 36 / 18 | — |
| workflow | 34 | +14 (`best_of`: plan, runtime, replay, result) |

R32 comparison: 2521 total (2420 default + 101 with `sqlite`); R33 is
2570 (2469 + 101) — **+49 tests**, all of them pinning a seam this round
wired up.

## Deferred to R34 (recorded, not dropped)

Compaction log-only events + `SurfaceOp.replace` checkpoint;
`assistant/chunk` live-resume (pi delta frames); `SessionEventMap`
extensibility; FsAdapter error boundary; `shutdownChildSession`
lifecycle refinements; `DeferredHandle` / cross-gateway
`OperationRequest` polling; deployment-level wiring so a configured
`AdaptiveRegistry` / `GroupedRegistry` reaches the projection (the seam
now exists); a scoring closure on `best_of`; provider auth flows
(PKCE / device-code / credential store); partial-JSON *display* for
mid-stream UIs (deliberately not a parse path).
