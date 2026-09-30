# Optimization report R74 — 2026-09-15

## What the audit found

R73 closed the LLM-judge gap. The audit then re-walked the
`traitclaw` and `pi` source trees for any port the workspace has
not absorbed and that is autonomously feasible (no maintainer
decision, no runtime-type addition, no breaking change).

The pi `agent-loop.ts` ships a `validateToolArguments(tool, args)`
call that runs every tool call's JSON arguments through the
tool's JSON Schema *before* dispatch, and synthesises an
`is_error` result listing field-level violations on mismatch
(so the model can self-correct in the same turn). Synthia's
`Tool::call` doc says *"Implementations should parse `input`
internally"* — the dispatcher's hot path does **not** validate
arguments; each tool's body is the source of truth.

That is the same shape the rest of Synthia has been moving away
from: `StructuredOutputTool` re-validates its own payload via
`synthia_core::validate_against_schema`; the eval grader's
`LlmJudgeMetric` validates its own provider reply. A registry
hook that does it once, for every tool, with a clean opt-in
switch, is the missing piece.

## What landed

- `ToolRegistry::with_argument_validation(bool)` —
  opt-in builder. Off by default so the hot path is unchanged
  for consumers that already validate inside `Tool::call` or
  trust the provider's tool-call arguments.
- `ToolRegistry::argument_validation_enabled()` — introspection
  for the new flag.
- `Plan::Invalid { name, violations }` arm in `run_stream` —
  a failed match synthesises an `is_error` `Result` listing
  every dotted-path violation, paid under the same read lock
  as the tool lookup so a misbehaving call costs a synthesized
  error Result instead of a spawned task that runs a tool it
  shouldn't.
- The error body format:
  ```
  invalid arguments for tool `shell` (2 violations):
  - cmd: expected required property present, found missing
  - timeout: expected type integer, found string
  ```
  matches `synthia_eval::metrics::parse_score`'s style: numbered
  list, dotted paths, the model's `is_error` path treats it as
  an instruction to retry.
- `Clone` carries the flag through (tested), so a deployment
  that builds a child registry from a validated parent does not
  silently lose the guarantee.
- 5 new tests in `synthia-tool --lib`:
  - `dispatch_passes_arguments_to_the_tool_when_validation_is_off`
    — default behaviour unchanged (the tool body runs even
    with a non-matching input, since validation is opt-in).
  - `dispatch_synthesises_schema_violation_error_when_validation_is_on`
    — missing `cmd` + wrong-typed `timeout` both surface as
    `is_error: true` with both violations in the body.
  - `dispatch_runs_the_tool_when_validation_passes` — the
    validator does not over-reject.
  - `clone_preserves_argument_validation_flag` — `Clone`
    contract.
  - `validation_passes_through_empty_schemas` —
    `validate_against_schema`'s permissive default for empty
    / non-object schemas is preserved through the registry.

The validator is `synthia_core::validate_against_schema`, the
same primitive `StructuredOutputTool` and `SchemaValidationMetric`
already use. No new crate, no new dependency, no new feature
flag.

## Verification

- `cargo test -p synthia-tool --lib` 283 passed (was 278, +5).
- `make ci` 6/6 green.
- `make test-unit` 2419 passed (was 2414, +5).
- `make examples` exit 0 with `CONSUMER-PROOF: OK` + `MVP-OK` +
  `BEST-OF-N-JUDGE: OK`.
- `make check-mvp-deps` and `make check-public-api-runtime`
  both green; no runtime type leaks into the new public method.
- The error body is a `ToolOutput::error(...)` with
  `is_error: Some(true)`, the same wire shape the rest of
  the registry emits — so a downstream consumer (the chat UI,
  the durable log, the eval runner) treats it like any other
  tool error and the model self-corrects in the same turn.

## Deferred

- The redaction side: the error body echoes the user-supplied
  `input` shape (so the model can fix it), which is the same
  trade-off `StructuredOutputTool` makes. A deployment that
  needs the error body redacted (PII) can wrap `Tool::call`
  in a hook that strips it from the message before it leaves
  the registry. Recorded as a follow-up — the LLM-judge
  prompt is the highest-leverage place to enforce that, and
  the hook seam (`HookStage::AfterToolExecute`) already exists.
- `synthia-server`'s `synthesize_tool_definitions` adapter: it
  can flip the registry flag on at boot, but `synthia-server`
  has its own pre-existing configuration knobs (the `tool_*`
  env vars documented in `DEPLOYMENT.md`) and adding a
  `tools.validate_arguments` knob is a server-config change,
  not a code change. R50.
