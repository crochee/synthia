# Synthia R29 — plan (2026-09-11)

> **STATUS: fully executed.** Phases A–F landed in commit
> `8f1821cf`; phases G–M landed in commit `aae5cb37` (see the
> `[Unreleased]` CHANGELOG section, "R29 (cont.)"). The
> remaining-phases contract, including the one deliberate
> deviation from this document (nested `[operations]` config
> rather than a bare `enable_operation_endpoint` bool, matching
> every other subsystem's config shape), lives in
> [`optimization-report-R29-remaining-contract.md`](optimization-report-R29-remaining-contract.md).
>
> Deviations recorded during execution:
>
> - Phase G: the clone's tool registry ships **empty** rather
>   than carrying one synthetic `agent` tool. Registering that
>   tool needs a peer-dispatch seam that does not exist yet, and
>   an empty registry is the honest R29 state — the test pins
>   "no builtins leak into the invisible turn".
> - Phase J: `CompactionStart/Summary/End.token` is a `String`
>   (`SurfaceToken` is the typed constructor), and the
>   lifecycle→event bridge is realised as a **loader-side**
>   `compaction_outcomes` view rather than an unused emitter
>   bridge.
> - Phase K: only `read` / `write` are tracked; shell-command
>   file inference is deliberately omitted as guesswork.
> - Phase L: the fold helper is `synthia_session::assemble_chunks`
>   (the `SessionSurface` type named here does not exist).
> - Phase M: the endpoint is
>   `POST /api/v1/chat/sessions/{id}/operation` — the
>   `/v1/runs/{agent}/…` prefix named here does not match
>   synthia's actual route tree.

> 4-reference absorption round: traitclaw + dsh + pi + pi-subagents.
> Goal: take the highest-leverage NEW patterns from the four
> references that synthia has NOT yet absorbed, and make them
> concrete in code, examples, and tests. Keep the round
> scoped — focus on items that are
>
> - **runtime-neutral** (no tokio/axum in lib public API; preserves
>   the R23 guarantee),
> - **leverage-amplifying** (delete code, not add it; or add
>   a primitive that future R30+ work builds on), and
> - **concretely testable** (a real test pins each item).
>
> Out of scope for R29 (will appear in R30+ if approved):
> - SubagentWorkflow scripting (pi-subagents §1) — too large for
>   one round, would need a new `synthia-workflow` crate and a
>   deterministic-prelude story.
> - TUI widgets (pi coding-agent) — synthia is HTTP, not TUI.
> - Full `SessionEventMap` declaration-merging via
>   TypeScript-style `interface`-extending — Rust has no such
>   facility; we ship a `SessionEventRegistry` plumbing as the
>   mechanical substitute.
> - DeferredHandle cross-gateway polling (pi §1) — needs a
>   real gateway; we ship the wire shape only.
> - Cross-extension RPC envelopes (pi-subagents §5) — synthia is
>   a single binary, no event bus; the HTTP gateway already
>   carries the equivalent reply envelope.
> - `StreamChunk` content-tagging changes — would break the
>   provider public API; tracked for R30.

## R29 deliverables

### Phase A — Tool output contract completes (dsh §8)

The R17 `Tool::output_definition()` is a default method on
`Tool` and returns a `ToolOutputDefinition` with
`render(args, value) → ContentBlock[]`. dsh's `ToolDefinition`
also carries `presentationMeta(args, value): JsonValue` and a
total `finalizeContent(args, outcome) → ContentBlock[]` that
runs on every outcome (including pipeline failures). The first
lets the same tool serve both the model and a UI without
leaking UI vocabulary into model-visible text; the second
enforces a tool-owned content bound on the failure path.

R29 work:

1. Add `presentation_meta(args, value) -> Option<JsonValue>` as
   a default method on `Tool` that returns `None`; have
   `ToolOutputDefinition` carry an optional
   `presentation_meta: Option<JsonValue>` field; wire
   `ToolRegistry::run_stream` to attach the value to the
   emitted `ToolOutput` (next to `render`).
2. Add `finalize_content(outcome: &ToolOutcome) -> ContentBlock[]`
   as a default method that returns a text-or-json block from
   the outcome's `output` field (preserving today's contract).
   Tools that need a different failure shape override.
3. Add a `ToolOutput::presentation` accessor returning
   `Option<&JsonValue>` (no-op when not set) so the HTTP layer
   can include it in the wire envelope for UI surfaces.

### Phase B — Per-scope tool restriction (dsh §10)

dsh's `ToolRestriction { allow?, deny? }` is applied per agent
scope; restrictions from all ancestor scopes intersect; the
scope's OWN registrations stay exempt so a delegated child
keeps the tools it answers through. Synthia has
`AgentBuilder::tools(vec)` (wholesale override) but no
per-scope allow/deny.

R29 work:

1. Add a `ToolRestriction` value type to `synthia-tool`:
   `allow: Option<Vec<String>>`, `deny: Vec<String>`,
   `is_relevant(name) -> bool` (allow-list excludes unlisted
   inherited tools; deny-list admits them; intersection across
   parents).
2. Add `ToolRegistry::restrict(&self, r: &ToolRestriction) -> Arc<ToolRegistry>`
   that returns a new registry that filters its visible
   tool list through the restriction. Implementations
   snapshot the tool list under `Arc<RwLock<...>>` so the
   restricted registry stays cheap to clone.
3. Add `AgentBuilder::tool_restriction(r: ToolRestriction)` to
   surface it on the assembly path.
4. Test: deny list applied at a parent scope is honored by a
   delegated child scope; own registrations stay visible.

### Phase C — Credential validation (dsh §6)

dsh's `normalizeApiKey(raw)` trims whitespace, validates
`/^[\x21-\x7E]+$/`, and fails with `ApiKeyRejection` distinct
from `MISSING_CREDENTIAL`/`INVALID_CREDENTIAL`. A
malformed credential should never reach undici/fetch.

R29 work:

1. Add `synthia_provider::credential::{normalize_api_key,
   classify_credential}` helpers. `normalize` returns
   `Result<String, CredentialError>` with two variants:
   `Empty`, `IllegalCharacters`. `classify` returns
   `Missing | Invalid | Valid` for the high-level
   pre-send gate.
2. Add a `synthia_core::Error::InvalidCredential { reason }`
   variant (deliberately NOT retryable).
3. Wire `OpenAIProvider` / `AnthropicProvider` constructors to
   run the classifier at construction time; a malformed
   credential fails fast with `Error::InvalidCredential`.

### Phase D — Provider replay state (dsh §7)

dsh's assistant message carries
`provenance.replayState?: JsonValue`; the runtime passes it
back to the adapter only when the same adapter instance owns
both historical and target routes. Lets a model call survive a
provider switch without losing Anthropic prompt-cache ids.

R29 work:

1. Add `CompletionResponse::replay_state: Option<serde_json::Value>`
   to `synthia_provider`; populated by the adapter when the
   provider returns a known cache id (Anthropic
   `cache_control` block, OpenAI `response_id`).
2. Add `CompletionRequest::replay_state: Option<serde_json::Value>`
   the agent loop threads through unchanged; the adapter
   consults it before sending (and drops it if the provider
   is different).
3. Test: provider receives its own replay_state on a no-op
   second call; provider receives `None` when the
   `replay_state` was minted by a different adapter.

### Phase E — Feature-gated tool schema (pi-subagents §7)

pi-subagents removes a JSON-schema field from the model's
view when the capability is not enabled. Today every synthia
tool ships its full schema every turn. A small primitive lets
tools compute their schema conditionally.

R29 work:

1. Add `synthia_tool::schema::ToolSchemaBuilder` (a thin
   builder wrapping `serde_json::Map`) with
   `.with(key, value)`, `.without(key)`, `.merge(other)`,
   `.build() -> serde_json::Value`.
2. Refactor `Tool::parameters()` to a default method that
   returns `schemars::schema_for!(Self::Input)` and exposes a
   `.with_conditional(features)` hook returning the schema.
   For the 5 builtins, expose
   `output_definition` already carries
   `ToolOutputDefinition::passthrough`; add a
   `ToolFeature`-gated helper on `ToolOutputDefinition` so
   `.with_feature(Feature::ImagePreview, true)` only attaches
   the `image_preview` presentation when the feature is on.
3. Add a smoke test exercising the schema builder + a
   `Tool` that omits a field conditionally.

### Phase F — Subagent invocation records (pi-subagents §6)

`invocation-config.ts:resolveAgentInvocationConfig` returns
`overridden: { model?, thinking? }` when caller params were
outranked by agent frontmatter. UIs render
`sonnet 4.6 · thinking: low (asked max)` instead of silently
substituting.

R29 work:

1. Add a 3-field tuple to
   `synthia_agent::events::AgentMeta`:
   `requested_model: Option<String>`,
   `requested_thinking: Option<String>`,
   `overrides_applied: bool` (true when the
   resolved `model`/`thinking` differ from
   `requested_model`/`requested_thinking`).
2. Wire `AgentBuilder` to record both pairs when
   `model_hint` is set vs unset (call side records
   `requested_model`; descriptor side records the
   resolved one).
3. Test: builder without `model_hint` → record has
   `requested_model = None`; builder with
   `model_hint = Some("gpt-5")` and a different
   descriptor `model` → record has both.

### Phase G — MentionClone (pi-subagents §2)

`mention-clone.ts`: fork the conversation into a throwaway
in-memory session, build the compaction-aware context, prompt
the clone with only the `Agent` tool, attribute the spawn to
the real session. Force background.

R29 work:

1. Add `synthia_agent::mention::MentionClone` —
   a helper that takes a `&dyn AgentContext` (a
   capability-shape trait we already have via
   `AgentRegistry` + `ToolRegistry`) and a
   `&AgentDescriptor`, builds a child `ReActAgent` whose
   tool registry contains only the `agent` tool, and
   returns a `DetachedAgent` handle.
2. Add `Router::mention_clone_mode` flag (default
   `false`); when `true`, mentions spawn via
   `MentionClone` instead of the leader's in-place
   delegation.
3. Test: clone receives only the `agent` tool; clone's
   tool list is disjoint from the leader's
   `ToolRegistry` set.

### Phase H — subagent_scheduler + JobStore (pi-subagents §3)

A clean cron + interval + once dispatcher. PID-locked
atomic JSON persistence at `<cwd>/.pi/subagent-schedules/`.
bypassQueue so scheduled fires cannot be deferred.

R29 work:

1. Add a new `synthia-scheduler` crate
   (3 modules: `Job` / `Scheduler` / `Store`).
2. `Job::Schedule { kind: Cron | Interval | Once, ... }`.
3. `Scheduler::tick() -> Vec<JobId>` to drive from any
   runtime (caller's `tokio::task::spawn_local` or
   `smol` equivalent; runtime-neutral).
4. `Store`: PID-locked file at
   `<root>/schedules/<job_id>.json`; load-on-boot,
   save-on-mutate; fcntl `flock` for atomic
   cross-instance writes.
5. Tests: store survives concurrent writes
   (one per PID); scheduler tick fires for a 100ms
   interval; once-job de-registers after fire.

### Phase I — session_shutdown lifecycle (pi-subagents)

`shutdownChildSession` race between
`runner.emit('session_shutdown', quit)` and a 3s
`setTimeout().unref()`. Without the emit, an
extension armed in `session_start` (timer callback) throws
`assertActive()` on its next tick.

R29 work:

1. Add a new `SessionLifecycle::Shutdown` event to
   `synthia_session::SessionEvent` enum (already
   typed since R6).
2. Emit `Shutdown` from `SessionController::close` after
   all child sessions have been signalled.
3. Add a 3-second `unref` timer to
   `SessionController::close` so a hung shutdown
   doesn't keep the process alive.
4. Test: session with 2 children → close → both
   received `Shutdown` event; the
   `CleanupTask` finishes within 3s.

### Phase J — surface-op replace for compaction (dsh §12)

Compaction is its own capability seam. The three events
`compaction/start`, `compaction/summary`, `compaction/end` are
log-only. The summary rides on a separate `user/message` with
`surfaceOp: { op: 'replace', start, end }` citing every
shadowed surface node. A crash mid-compaction produces an
orphaned `start` (detectable) rather than a `compaction/end`
that falsely claims success.

R29 work:

1. Add three new variants to
   `synthia_session::SessionEvent`:
   `CompactionStart { token: SurfaceToken }`,
   `CompactionSummary { token, summary: String,
   shadowed: Vec<u64> }`, `CompactionEnd { token,
   outcome: CompactionOutcome }`.
2. Add `SurfaceOp` and `SurfaceToken` value types to
   `synthia_session` (`replace { start, end, source }`).
3. Wire `SummarizingContextManager` to emit Start before
   the LLM call, Summary after success, End on
   commit/recovery; on failure, emit End with
   `CompactionOutcome::Failed`.
4. Test: a crash before End leaves an orphaned Start;
   the loader can detect it and surface as
   `CompactionOutcome::Interrupted`.

### Phase K — CompactionDetails (pi §13)

pi's `CompactionDetails { readFiles: Vec<String>,
modifiedFiles: Vec<String> }` is stored on the
`CompactionEntry`. Subsequent compactions read the previous
entry's details and pass them to the LLM so it knows
"this is what touched disk in the most recent run".

R29 work:

1. Extend
   `synthia_context::SummarizingContextManager`'s
   `CompactionSettings` with
   `read_files: Vec<PathBuf>` and
   `modified_files: Vec<PathBuf>` (paths recorded by the
   agent loop before the LLM call, passed to the
   summarizer).
2. Wire `ReActLoop` to accumulate paths touched in the
   just-closed iteration (read tool / write tool / shell
   with file mutation) and pass them in.
3. Test: after a run that read `a.rs` and wrote
   `b.rs`, the next compaction's `CompactionDetails`
   carries both paths.

### Phase L — AssistantMessage replay-state + assistant/chunk (dsh §20)

`assistant/chunk` raw-event preservation for replay
fidelity. Every raw `StreamChunk` is durably logged; the
loop logs raw chunks AND feeds them through a
`BlockAssembler` to produce the assembled
`assistant/message` for derived history.

R29 work:

1. Add `synthia_session::SessionEvent::AssistantChunk { seq: u64, delta: String, finish_reason: Option<FinishReason> }`.
2. Wire the agent loop to emit one `AssistantChunk` per
   provider stream delta (alongside the existing
   `assistant_message` event).
3. Add a fold helper
   `SessionSurface::assemble_chunks(events) -> Vec<AssistantMessage>`
   that joins adjacent chunks by `seq` and re-emits the
   derived `assistant/message`.
4. Test: 5-chunk stream → exactly 5
   `AssistantChunk` events + 1 assembled
   `AssistantMessage`; the fold helper reconstructs the
   same text.

### Phase M — OperationRequest wire union (pi §5)

pi's `OperationRequest` discriminated union
(`prompt | skill | prompt_template | compaction | navigation`).
The HTTP `POST /v1/runs/{agent}/prompt` gains the
discriminated union at the wire, picking the right
operation from the `kind` tag.

R29 work:

1. Add a `synthia_server::api::operation::OperationRequest`
   sum type with
   `Prompt { text | message[] }`, `Skill { name, args }`,
   `PromptTemplate { name, vars }`,
   `Compaction { force: bool }`, `Navigation { target,
   args }`.
2. Wire `POST /v1/runs/{agent}/operation` (new) to dispatch
   to the right existing route; preserve the old
   `POST /v1/runs/{agent}/prompt` for backward compat.
3. Add a server config flag `enable_operation_endpoint: bool`
   (default `false`); when off, the new endpoint returns 404.
4. Test: round-trip each variant → correct downstream route.

## Verification (per crate, never --workspace one-shot)

```bash
make fmt-rust
make lint-rust
# per-crate tests:
cargo test -p synthia-core
cargo test -p synthia-provider
cargo test -p synthia-tool
cargo test -p synthia-session
cargo test -p synthia-agent
cargo test -p synthia-steering
cargo test -p synthia-context
cargo test -p synthia-skill
cargo test -p synthia-attachment
cargo test -p synthia-mcp
cargo test -p synthia-rag
cargo test -p synthia-server
cargo test -p synthia-telemetry
cargo test -p test-support
# examples
for ex in assemble_from_scratch assemble_with_provider_profile \
          assemble_with_skills assemble_with_tier_steering \
          assemble_with_builder grounded_agent \
          register_remote_tools fan_out_with_group_join; do
  cargo run --example $ex -p synthia-agent
done
# external consumer
(cd docs/examples/external-consumer && cargo run)
# server boot
synthia-server --directory <ws> --port 18080 &
PID=$!
sleep 1
curl -s http://127.0.0.1:18080/livez
curl -s http://127.0.0.1:18080/readyz
kill $PID
```

## Boundaries (not claimed)

- No live LLM turn in this round (R29 is library work; the
  test suite uses `FakeProvider`).
- The new `synthia-scheduler` crate is a library; the server
  is not wired to it (intentional — that is an R30 concern).
- `OperationRequest` does not replace the existing
  `POST /v1/runs/{agent}/prompt`; both endpoints coexist
  behind a feature flag.
- The non-exhaustive audit is a one-pass sweep; future enum
  additions must keep the discipline.
